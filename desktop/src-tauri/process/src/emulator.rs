//! Portable emulators receive literal argv values and private, first-run config
//! files. Never write to the emulator package or the user's home directory.
use crate::{ParsedCommand, error::ProcessError};
use database::models::data::{EmulatorLaunchMode, LaunchConfiguration};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

fn invalid(message: &str) -> ProcessError {
    ProcessError::InvalidArguments(message.into())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path
            .chars()
            .any(|c| c == '\\' || c == ':' || c.is_control())
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

// Create/check each directory separately: create_dir_all alone would follow a
// symlink planted inside a game archive and escape the isolated directory.
fn directory(root: &Path, relative: &str) -> Result<PathBuf, ProcessError> {
    if !safe_path(relative) {
        return Err(invalid("Unsafe emulator configuration path."));
    }
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(invalid(
                "Emulator configuration directories must not be symlinks.",
            ));
        }
    }
    Ok(path)
}

// One pass, not chained replace: braces in an actual filename are literal.
fn expand(input: &str, values: &[(&str, String)], json: bool) -> String {
    let mut output = String::new();
    let mut rest = input;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let found = values.iter().find_map(|(name, value)| {
            let token = format!("{{{name}}}");
            if rest.starts_with(&token) {
                return Some((token.len(), value.clone()));
            }
            let token = format!("{{{name}:json}}");
            if json && rest.starts_with(&token) {
                return Some((
                    token.len(),
                    serde_json::to_string(value).expect("string serialization"),
                ));
            }
            None
        });
        if let Some((length, value)) = found {
            output.push_str(&value);
            rest = &rest[length..];
        } else {
            output.push('{');
            rest = &rest[1..];
        }
    }
    output.push_str(rest);
    output
}

pub fn prepare(
    launch: &LaunchConfiguration,
    target: &ParsedCommand,
    game_dir: &Path,
    emulator_dir: &Path,
    version_id: &str,
) -> Result<ParsedCommand, ProcessError> {
    let game_dir = dunce::canonicalize(game_dir)?;
    let emulator_dir = dunce::canonicalize(emulator_dir)?;
    let profile = launch.emulator_profile.as_ref();
    if let Some(profile) = profile {
        match profile.mode {
            EmulatorLaunchMode::Cli if !launch.command.contains("{rom}") => {
                return Err(invalid("CLI emulator commands must contain {rom}."));
            }
            EmulatorLaunchMode::Ui if launch.command.contains("{rom}") => {
                return Err(invalid(
                    "UI-first emulator commands must not contain {rom}.",
                ));
            }
            _ => {}
        }
        if profile.config_files.len() > 32
            || (!profile.config_files.is_empty() && !launch.command.contains("{configDir}"))
        {
            return Err(invalid("Invalid emulator configuration profile."));
        }
    }
    let rom = game_dir.join(&target.command);
    if !rom.exists() {
        return Err(invalid(
            "The configured emulator game file or folder is missing.",
        ));
    }
    let config_dir = directory(
        &game_dir,
        &format!(".drop-emulator/{version_id}/{}", launch.launch_id),
    )?;
    let values = [
        ("rom", rom.to_string_lossy().into_owned()),
        ("gameDir", game_dir.to_string_lossy().into_owned()),
        ("emulatorDir", emulator_dir.to_string_lossy().into_owned()),
        ("configDir", config_dir.to_string_lossy().into_owned()),
    ];
    if let Some(profile) = profile {
        for file in &profile.config_files {
            if !safe_path(&file.path) || file.content.len() > 262144 {
                return Err(invalid("Unsafe emulator configuration template."));
            }
            let (parent, filename) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
            let parent = if parent.is_empty() {
                config_dir.clone()
            } else {
                directory(&config_dir, parent)?
            };
            let path = parent.join(filename);
            // create_new refuses existing files, symlinks and hard links. A
            // user's edited config is authoritative after initial generation.
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut handle) => {
                    handle.write_all(expand(&file.content, &values, true).as_bytes())?
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    let meta = fs::symlink_metadata(&path)?;
                    if !meta.is_file() || meta.file_type().is_symlink() {
                        return Err(invalid(
                            "Emulator configuration files must not be symlinks.",
                        ));
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    let mut command = ParsedCommand::parse(launch.command.clone())?;
    command.command = expand(&command.command, &values, false);
    command.make_absolute(emulator_dir);
    command.args = command
        .args
        .iter()
        .map(|arg| expand(arg, &values, false))
        .collect();
    command.env = command
        .env
        .iter()
        .map(|env| expand(env, &values, false))
        .collect();
    command.env.extend(target.env.clone());
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{
        models::data::{EmulatorConfigFile, EmulatorProfile},
        platform::Platform,
    };

    fn launch(mode: EmulatorLaunchMode, command: &str) -> LaunchConfiguration {
        LaunchConfiguration {
            proton_compatible: false,
            launch_id: "launch".into(),
            name: "Play".into(),
            command: command.into(),
            platform: Platform::Linux,
            umu_id_override: None,
            emulator: None,
            emulator_profile: Some(EmulatorProfile {
                mode,
                config_files: vec![],
            }),
        }
    }

    #[test]
    fn cli_paths_remain_literal_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let rom = dir.path().join("Player's {gameDir}.rom");
        fs::write(&rom, "rom").unwrap();
        let command = prepare(
            &launch(EmulatorLaunchMode::Cli, "emulator --run '{rom}'"),
            &ParsedCommand::parse("\"Player's {gameDir}.rom\"".into()).unwrap(),
            dir.path(),
            dir.path(),
            "v1",
        )
        .unwrap();
        assert_eq!(command.args, ["--run", rom.to_str().unwrap()]);
        let parsed = ParsedCommand::parse(command.reconstruct()).unwrap();
        assert_eq!(parsed.args[1], rom.to_str().unwrap());
    }

    #[test]
    fn ui_templates_are_isolated_json_safe_and_preserve_user_edits() {
        let dir = tempfile::tempdir().unwrap();
        let mut launch = launch(
            EmulatorLaunchMode::Ui,
            "emulator --config '{configDir}/settings.json'",
        );
        launch
            .emulator_profile
            .as_mut()
            .unwrap()
            .config_files
            .push(EmulatorConfigFile {
                path: "settings.json".into(),
                content: "{\"games\": [{gameDir:json}]}".into(),
            });
        let target = ParsedCommand::parse(".".into()).unwrap();
        let command = prepare(&launch, &target, dir.path(), dir.path(), "v1").unwrap();
        let path = &command.args[1];
        let json: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(json["games"][0], dir.path().to_str().unwrap());
        fs::write(path, "user settings").unwrap();
        prepare(&launch, &target, dir.path(), dir.path(), "v1").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "user settings");
    }

    #[test]
    fn rejects_cross_platform_traversal_and_invalid_modes() {
        for path in [
            "../config",
            "/config",
            "C:/config",
            "a\\config",
            "a//b",
            "a/./b",
            "a/../b",
            "x\0",
        ] {
            assert!(!safe_path(path), "{path}");
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(
            prepare(
                &launch(EmulatorLaunchMode::Cli, "emulator"),
                &ParsedCommand::parse(".".into()).unwrap(),
                dir.path(),
                dir.path(),
                "v1"
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_config_directories() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join(".drop-emulator")).unwrap();
        assert!(
            prepare(
                &launch(EmulatorLaunchMode::Ui, "emulator"),
                &ParsedCommand::parse(".".into()).unwrap(),
                dir.path(),
                dir.path(),
                "v1"
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}
