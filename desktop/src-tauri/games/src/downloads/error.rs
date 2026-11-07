use std::fmt::Display;

use serde_with::SerializeDisplay;

#[derive(SerializeDisplay)]
pub enum LibraryError {
    MetaNotFound(String),
    VersionNotFound(String),
    RequiredByGames,
    Busy,
    VersionChanged,
}
impl Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LibraryError::RequiredByGames => "Installed games require this emulator version. Review the affected games and confirm the uninstall again.".to_owned(),
                LibraryError::Busy => "Stop affected games and wait for current operations to finish before uninstalling this application.".to_owned(),
                LibraryError::VersionChanged => "The installed version changed. Refresh the emulator list before uninstalling it.".to_owned(),
                LibraryError::MetaNotFound(id) => {
                    format!(
                        "Could not locate any installed version of game ID {id} in the database"
                    )
                }
                LibraryError::VersionNotFound(game_id) => {
                    format!(
                        "Could not locate any installed version  for game id {game_id} in the database"
                    )
                }
            }
        )
    }
}
