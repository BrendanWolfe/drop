//! Side-effect-free command/template and working-directory resolution shared
//! with the normal launcher. Steam adds strict checks; it never guesses an exe.
use crate::{
    ParsedCommand, error::ProcessError, format::DropFormatArgs, process_handlers::WindowsLauncher,
    process_manager::ProcessHandler,
};
use database::{
    Database, GameDownloadStatus, GameVersion, models::data::InstalledGameType, platform::Platform,
};
use dynfmt::Format;
use std::path::{Path, PathBuf};

pub struct ResolvedCommand {
    pub command: ParsedCommand,
    pub working_directory: PathBuf,
}

pub fn finish_command(
    target: String,
    install_dir: &String,
    version: &GameVersion,
    local_directory: Option<&Path>,
) -> Result<ResolvedCommand, ProcessError> {
    let mut parsed = ParsedCommand::parse(target.clone())?;
    let executable = parsed.command.clone();
    parsed.make_absolute(install_dir.into());
    let format_args = DropFormatArgs::new(target, install_dir, &executable, parsed.command, None);
    let target = dynfmt::SimpleCurlyFormat
        .format(&version.user_configuration.launch_template, &format_args)
        .map_err(|e| ProcessError::FormatError(e.to_string()))?
        .to_string();
    let target = if local_directory.is_some() {
        target
    } else {
        dynfmt::SimpleCurlyFormat
            .format(&target, format_args)
            .map_err(|e| ProcessError::FormatError(e.to_string()))?
            .to_string()
    };
    let mut command = ParsedCommand::parse(target)?;
    let install = PathBuf::from(install_dir);
    command.make_command_absolute_if_local(&install);
    let working_directory = local_directory.map(Path::to_path_buf).unwrap_or_else(|| {
        let path = Path::new(&command.command);
        match path.parent() {
            Some(parent)
                if path.is_absolute() && parent.starts_with(&install) && parent != install =>
            {
                parent.to_path_buf()
            }
            _ => install,
        }
    });
    Ok(ResolvedCommand {
        command,
        working_directory,
    })
}

pub struct SteamLaunch {
    pub resolved: ResolvedCommand,
    pub launch_id: String,
    pub launch_name: String,
    pub configuration: String,
}

pub fn resolve_steam(
    db: &Database,
    id: &str,
    launch_id: Option<&str>,
    host: Platform,
) -> Result<SteamLaunch, String> {
    if host != Platform::Windows {
        return Err("Steam integration is available on Windows only.".into());
    }
    if db
        .applications
        .transient_statuses
        .keys()
        .any(|meta| meta.id == id)
    {
        return Err("Stop the game or wait for its download/update to finish first.".into());
    }
    let Some(GameDownloadStatus::Installed {
        install_type: InstalledGameType::Installed,
        version_id,
        ..
    }) = db.applications.game_statuses.get(id)
    else {
        return Err("This game must be fully installed with setup complete.".into());
    };
    let meta = db
        .applications
        .installed_game_version
        .get(id)
        .ok_or("Game is not installed.")?;
    let resolved_install_dir = db
        .applications
        .resolve_install_path(meta)?
        .to_string_lossy()
        .into_owned();
    let install_dir = &resolved_install_dir;
    if meta.target_platform != Platform::Windows {
        return Err("Steam shortcuts require a Windows game.".into());
    }
    let version = db
        .applications
        .game_versions
        .get(version_id)
        .ok_or("Installed launch configuration is missing.")?;
    let options = version
        .launches
        .iter()
        .filter(|launch| launch.platform == Platform::Windows)
        .collect::<Vec<_>>();
    let launch = match launch_id {
        Some(id) => options.iter().find(|launch| launch.launch_id == id).copied().ok_or("The selected launch option no longer exists. Select another option and update the shortcut.")?,
        None if options.len() == 1 => options[0],
        _ => return Err("Choose a launch option for the Steam shortcut.".into()),
    };
    if launch.emulator.is_some() {
        return Err("This game requires an emulator/dependency launch configuration, which direct Steam shortcuts do not support.".into());
    }
    let config = &version.user_configuration;
    if !matches!(
        config.override_handler.as_deref(),
        None | Some("windows-auto" | "windows-direct")
    ) || config.override_proton_path.is_some()
    {
        return Err("This game uses a wrapper or compatibility handler. Direct Steam shortcuts cannot represent it.".into());
    }
    // Appending arguments and Drop placeholders can be represented; prepending
    // a command/wrapper, or replacing the command, cannot be exported faithfully.
    if !config.launch_template.starts_with("{}")
        || config.launch_template.matches("{}").count() != 1
    {
        return Err("This custom launch template uses a wrapper or replaces the executable. Direct Steam shortcuts are not supported.".into());
    }
    let parsed = ParsedCommand::parse(launch.command.clone()).map_err(|e| e.to_string())?;
    if !parsed.env.is_empty() {
        return Err(
            "Custom environment variables cannot be represented by a direct Steam shortcut.".into(),
        );
    }
    if !crate::process_handlers::steam_executable_supported(Path::new(&parsed.command)) {
        return Err("This game requires a launch script or a non-.exe launcher. Direct Steam shortcuts are not supported for this configuration yet.".into());
    }
    let target = WindowsLauncher
        .create_launch_process(meta, parsed.reconstruct(), version, install_dir, db)
        .map_err(|e| e.to_string())?;
    let local = db
        .applications
        .local_games
        .get(id)
        .map(|local| local.working_directory.as_path());
    let resolved =
        finish_command(target, install_dir, version, local).map_err(|e| e.to_string())?;
    if !resolved.command.env.is_empty() {
        return Err("Custom environment variables are not supported.".into());
    }
    if !crate::process_handlers::steam_executable_supported(Path::new(&resolved.command.command))
        || resolved
            .command
            .args
            .iter()
            .any(|arg| arg.to_ascii_lowercase().contains("%command%"))
    {
        return Err("This launch requires an interpreter, wrapper, or Steam command substitution and cannot be exported directly.".into());
    }
    if !Path::new(&resolved.command.command).is_absolute()
        || !Path::new(&resolved.command.command).is_file()
    {
        return Err(
            "The configured executable does not exist. Repair or update the game first.".into(),
        );
    }
    if !resolved.working_directory.is_absolute() || !resolved.working_directory.is_dir() {
        return Err("The configured working directory does not exist.".into());
    }
    if resolved.command.command.contains('"')
        || resolved.command.command.contains('\0')
        || resolved.working_directory.to_string_lossy().contains('\0')
    {
        return Err("Invalid executable or working-directory path.".into());
    }
    Ok(SteamLaunch {
        resolved,
        launch_id: launch.launch_id.clone(),
        launch_name: launch.name.clone(),
        configuration: format!(
            "{:?}",
            (
                &config.launch_template,
                &config.override_handler,
                &config.override_proton_path
            )
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("A Game");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let exe = root.join("bin/play.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        let mut db = Database::new(&root, None, dir.path().join("cache"));
        let command = ParsedCommand {
            env: vec![],
            command: exe.to_string_lossy().into_owned(),
            args: vec!["Player One".into(), "日本語".into()],
        }
        .reconstruct();
        games::local::register(
            &mut db,
            database::models::data::LocalGame {
                id: "local-test".into(),
                name: "A Game".into(),
                root: root.clone(),
                launcher: Some(exe),
                arguments: "'Player One' 日本語".into(),
                working_directory: root,
                platform: Platform::Windows,
                playtime_seconds: 0,
                archive: None,
            },
            Some(command),
        );
        (dir, db)
    }
    fn resolve(db: &Database) -> Result<SteamLaunch, String> {
        resolve_steam(db, "local-test", Some("local-test"), Platform::Windows)
    }
    #[test]
    fn shares_nested_working_directory_rules_and_local_overrides() {
        let (_dir, mut db) = fixture();
        let local = resolve(&db).unwrap();
        assert_eq!(
            local.resolved.working_directory,
            db.applications.local_games["local-test"].root
        );
        assert_eq!(local.resolved.command.args, ["Player One", "日本語"]);
        db.applications.local_games.remove("local-test");
        let server = resolve(&db).unwrap();
        assert_eq!(
            server.resolved.working_directory,
            Path::new(&server.resolved.command.command)
                .parent()
                .unwrap()
        );
    }
    #[test]
    fn resolves_relative_exe_and_supported_template_arguments() {
        let (_dir, mut db) = fixture();
        db.applications.local_games.remove("local-test");
        let version = db.applications.game_versions.get_mut("local-test").unwrap();
        version.launches[0].command = "bin/play.exe --fullscreen".into();
        version.user_configuration.launch_template = "{} --name 'Player One'".into();
        let launch = resolve(&db).unwrap();
        assert!(Path::new(&launch.resolved.command.command).is_absolute());
        assert_eq!(
            launch.resolved.command.args,
            ["--fullscreen", "--name", "Player One"]
        );
    }
    #[test]
    fn refuses_scripts_wrappers_env_emulators_missing_files_and_wrong_platform() {
        let (_dir, db) = fixture();
        assert!(resolve_steam(&db, "local-test", None, Platform::Linux).is_err());
        for command in ["run.bat", "powershell.exe run.ps1", "A=one bin/play.exe"] {
            let mut db = db.clone();
            db.applications
                .game_versions
                .get_mut("local-test")
                .unwrap()
                .launches[0]
                .command = command.into();
            assert!(resolve(&db).is_err(), "{command}");
        }
        for template in ["wrapper {}", "{exe}", "{} {}"] {
            let mut db = db.clone();
            db.applications
                .game_versions
                .get_mut("local-test")
                .unwrap()
                .user_configuration
                .launch_template = template.into();
            assert!(resolve(&db).is_err());
        }
        let mut wrapped = db.clone();
        wrapped
            .applications
            .game_versions
            .get_mut("local-test")
            .unwrap()
            .user_configuration
            .override_handler = Some("windows-cmd".into());
        assert!(resolve(&wrapped).is_err());
        let mut emulator = db.clone();
        emulator
            .applications
            .game_versions
            .get_mut("local-test")
            .unwrap()
            .launches[0]
            .emulator = Some(database::models::data::LaunchConfigurationEmulator {
            launch_id: "emu".into(),
            game_id: "emu".into(),
            version_id: "emu".into(),
        });
        assert!(resolve(&emulator).is_err());
        let mut setup = db.clone();
        if let Some(GameDownloadStatus::Installed { install_type, .. }) =
            setup.applications.game_statuses.get_mut("local-test")
        {
            *install_type = InstalledGameType::SetupRequired;
        }
        assert!(resolve(&setup).is_err());
        let mut missing = db.clone();
        missing
            .applications
            .game_versions
            .get_mut("local-test")
            .unwrap()
            .launches[0]
            .command = "does-not-exist.exe".into();
        assert!(resolve(&missing).is_err());
        let mut wrong = db.clone();
        wrong
            .applications
            .installed_game_version
            .get_mut("local-test")
            .unwrap()
            .target_platform = Platform::Linux;
        assert!(resolve(&wrong).is_err());
    }
    #[test]
    fn requires_explicit_option_and_preserves_local_braces() {
        let (_dir, mut db) = fixture();
        let mut extra = db.applications.game_versions["local-test"].launches[0].clone();
        extra.launch_id = "alternate".into();
        db.applications
            .game_versions
            .get_mut("local-test")
            .unwrap()
            .launches
            .push(extra);
        assert!(resolve_steam(&db, "local-test", None, Platform::Windows).is_err());
        assert!(resolve(&db).is_ok());
        assert!(resolve_steam(&db, "local-test", Some("removed"), Platform::Windows).is_err());
        let version = &db.applications.game_versions["local-test"];
        let target = ParsedCommand {
            env: vec![],
            command: "Game {Deluxe}/play.exe".into(),
            args: vec![],
        }
        .reconstruct();
        let resolved = finish_command(
            target,
            &"/games".into(),
            version,
            Some(Path::new("/custom")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(resolved.command.command.contains("{Deluxe}"));
        assert_eq!(resolved.working_directory, Path::new("/custom"));
    }
}
