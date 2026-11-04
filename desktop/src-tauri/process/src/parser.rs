use std::path::{Path, PathBuf};

use log::info;

use crate::error::ProcessError;
use database::platform::Platform;

#[derive(Debug)]
pub struct ParsedCommand {
    pub env: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
}

impl ParsedCommand {
    pub fn parse(raw: String) -> Result<Self, ProcessError> {
        let parts =
            shell_words::split(&raw).map_err(|e| ProcessError::InvalidArguments(e.to_string()))?;
        let args = parts
            .iter()
            .position(|v| !environment_assignment(v))
            .ok_or(ProcessError::InvalidArguments(
                "Cannot parse launch".to_owned(),
            ))?;
        let env = &parts[0..args];
        let command = parts[args].clone();
        let args = &parts[(args + 1)..];

        Ok(Self {
            args: args.to_vec(),
            command,
            env: env.to_vec(),
        })
    }

    pub fn make_absolute(&mut self, base: PathBuf) {
        self.command = base
            .join(self.command.clone())
            .to_string_lossy()
            .to_string();
    }

    /// A picked file is literal, not shell syntax. Unix scripts may use their
    /// shebang interpreter even when a ZIP did not preserve executable bits.
    pub fn for_file(
        path: &Path,
        arguments: &str,
        platform: Platform,
    ) -> Result<Self, ProcessError> {
        use std::io::Read;
        let mut parsed = Self::parse(format!("launcher {arguments}"))?;
        parsed.command = path.to_string_lossy().into_owned();
        if platform != Platform::Windows {
            let mut buffer = [0u8; 4096];
            let length = std::fs::File::open(path)?.read(&mut buffer)?;
            let interpreter = if buffer[..length].starts_with(b"#!") {
                let end = buffer[..length]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .unwrap_or(length);
                let line = std::str::from_utf8(&buffer[2..end]).map_err(|_| {
                    ProcessError::InvalidArguments("Invalid script interpreter".into())
                })?;
                shell_words::split(line.trim())
                    .map_err(|e| ProcessError::InvalidArguments(e.to_string()))?
            } else if path.extension().is_some_and(|extension| extension == "sh") {
                vec!["/bin/sh".into()]
            } else {
                vec![]
            };
            if let Some(command) = interpreter.first() {
                let mut args = interpreter[1..].to_vec();
                args.push(parsed.command);
                args.extend(parsed.args);
                parsed.command = command.clone();
                parsed.args = args;
            }
        }
        Ok(parsed)
    }

    pub fn make_command_absolute_if_local(&mut self, base: &Path) {
        let candidate = base.join(&self.command);
        if candidate.is_file() {
            info!(
                "resolved local command '{}' to absolute path '{}'",
                self.command,
                candidate.display()
            );
            self.command = candidate.to_string_lossy().to_string();
        } else {
            info!(
                "command '{}' is not a local file in '{}', leaving as-is for PATH resolution",
                self.command,
                base.display()
            );
        }
    }

    pub fn ensure_executable(&self) -> Result<(), ProcessError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Ok(metadata) = std::fs::metadata(&self.command) {
                let is_executable =
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0;
                if !is_executable {
                    return Err(ProcessError::NotExecutable(self.command.clone()));
                }
            }
        }

        Ok(())
    }

    pub fn reconstruct(self) -> String {
        let mut v = vec![];
        v.extend(self.env);
        v.extend_one(self.command);
        v.extend(self.args);
        shell_words::join(v)
    }
}

pub struct LaunchParameters(pub ParsedCommand, pub PathBuf);

fn environment_assignment(value: &str) -> bool {
    let Some((name, _)) = value.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name.chars().enumerate().all(|(index, ch)| {
            ch == '_' || ch.is_ascii_alphabetic() || (index > 0 && ch.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picked_scripts_use_their_interpreter_without_changing_permissions() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("My launch script.sh");
        std::fs::write(&path, "#!/bin/sh\nprintf 'test'\n").unwrap();
        let parsed = ParsedCommand::for_file(&path, "--name 'Player One'", Platform::Linux)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed.command, "/bin/sh");
        assert_eq!(
            parsed.args,
            [
                path.to_string_lossy().into_owned(),
                "--name".into(),
                "Player One".into()
            ]
        );
    }

    #[test]
    fn literal_paths_and_arguments_round_trip() {
        let command = ParsedCommand {
            env: vec![],
            command: "/Games/My Game=Deluxe {edition}/play.exe".into(),
            args: vec!["--name".into(), "Player One".into(), "O'Brien".into()],
        };
        let parsed = ParsedCommand::parse(command.reconstruct()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed.command, "/Games/My Game=Deluxe {edition}/play.exe");
        assert_eq!(parsed.args, ["--name", "Player One", "O'Brien"]);
        assert!(parsed.env.is_empty());
    }

    #[test]
    fn preserves_environment_assignments_with_equals_in_their_values() {
        let parsed = ParsedCommand::parse("A=one=two '/Games/Game=Deluxe/play'".into())
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed.env, ["A=one=two"]);
        assert_eq!(parsed.command, "/Games/Game=Deluxe/play");
    }
}
