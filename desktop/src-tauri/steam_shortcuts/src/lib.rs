//! Steam's private shortcut format. No OS discovery or executable launching here.
#[cfg(test)]
mod tests;
pub mod vdf;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Error, Read, Result, Write},
    path::{Path, PathBuf},
};
use vdf::{Entry, Value};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportedLaunch {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub launch_id: String,
    pub configuration: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
    pub key: String,
    pub game_id: String,
    pub source: String,
    pub steam_root: PathBuf,
    pub profile_id: String,
    pub app_id: u32,
    /// Never expose a numeric u64 GameID to JavaScript.
    pub game_id64: String,
    pub ownership_token: String,
    pub exported: ExportedLaunch,
    pub exported_name: String,
    pub prefer_steam: bool,
}
pub fn game_id(app_id: u32) -> String {
    (((app_id as u64) << 32) | 0x0200_0000).to_string()
}
/// Windows CommandLineToArgvW/MSVC quoting, not Unix shell quoting.
pub fn quote_argument(value: &str) -> String {
    if !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c == '"') {
        return value.into();
    }
    let mut result = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        result.push(ch);
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}
pub fn launch_options(args: &[String]) -> String {
    args.iter()
        .map(|arg| quote_argument(arg))
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn executable_field(path: &str) -> String {
    format!("\"{path}\"")
}
pub fn initial_app_id(executable: &str, name: &str) -> u32 {
    crc32fast::hash(format!("{}{name}", executable_field(executable)).as_bytes()) | 0x8000_0000
}
pub fn shortcut_path(root: &Path, profile_id: &str) -> Result<PathBuf> {
    if profile_id.is_empty() || !profile_id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::other("Invalid Steam profile"));
    }
    Ok(root
        .join("userdata")
        .join(profile_id)
        .join("config/shortcuts.vdf"))
}
pub fn read_source(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = vec![];
            file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 32 * 1024 * 1024 {
                return Err(Error::other("Steam shortcut file is too large"));
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
pub struct Shortcuts {
    entries: Vec<Entry>,
}
impl Shortcuts {
    pub fn parse(bytes: Option<&[u8]>) -> Result<Self> {
        let entries = match bytes {
            None => vec![Entry::new("shortcuts", Value::Object(vec![]))],
            Some(bytes) => vdf::decode(bytes)?,
        };
        if !matches!(vdf::get(&entries, "shortcuts"), Some(Value::Object(_))) {
            return Err(Error::other(
                "Missing Steam shortcuts root; original left untouched",
            ));
        }
        let result = Self { entries };
        let mut ids = std::collections::HashSet::new();
        for item in result.items() {
            let Value::Object(fields) = &item.value else {
                return Err(Error::other("Invalid Steam shortcut entry"));
            };
            let id = vdf::number(fields, "appid")
                .ok_or_else(|| Error::other("Steam shortcut has no valid AppID"))?;
            if !ids.insert(id) {
                return Err(Error::other("Duplicate Steam AppID"));
            }
        }
        Ok(result)
    }
    fn items(&self) -> &[Entry] {
        match vdf::get(&self.entries, "shortcuts").unwrap() {
            Value::Object(entries) => entries,
            _ => unreachable!(),
        }
    }
    fn items_mut(&mut self) -> &mut Vec<Entry> {
        match &mut self
            .entries
            .iter_mut()
            .find(|e| e.key.eq_ignore_ascii_case(b"shortcuts"))
            .unwrap()
            .value
        {
            Value::Object(entries) => entries,
            _ => unreachable!(),
        }
    }
    fn fields(&self, app_id: u32) -> Option<&Vec<Entry>> {
        self.items().iter().find_map(|entry| match &entry.value {
            Value::Object(fields) if vdf::number(fields, "appid") == Some(app_id) => Some(fields),
            _ => None,
        })
    }
    pub fn contains(&self, app_id: u32) -> bool {
        self.fields(app_id).is_some()
    }
    pub fn allocate_app_id(&self, initial: u32, reserved: &[u32]) -> Result<u32> {
        let mut used = reserved
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        for entry in self.items() {
            if let Value::Object(fields) = &entry.value
                && let Some(id) = vdf::number(fields, "appid")
            {
                used.insert(id);
            }
        }
        let mut candidate = initial | 0x8000_0000;
        for _ in 0..=used.len() {
            if !used.contains(&candidate) {
                return Ok(candidate);
            }
            candidate = candidate.wrapping_add(1) | 0x8000_0000;
        }
        Err(Error::other("No available Steam shortcut AppID"))
    }
    fn owns(fields: &[Entry], registration: &Registration) -> bool {
        let tag = format!("Drop:{}", registration.ownership_token);
        match vdf::get(fields, "tags") {
            Some(Value::Object(tags)) => tags.iter().any(|entry| entry.value == vdf::text(&tag)),
            _ => false,
        }
    }
    pub fn verify(&self, registration: &Registration) -> Result<bool> {
        let Some(fields) = self.fields(registration.app_id) else {
            return Ok(false);
        };
        if !Self::owns(fields, registration) {
            return Err(Error::other(
                "Shortcut ownership no longer matches. Drop will not modify this entry.",
            ));
        }
        Ok(true)
    }
    pub fn matches(&self, registration: &Registration, launch: &ExportedLaunch) -> Result<bool> {
        if !self.verify(registration)? {
            return Ok(false);
        }
        let fields = self.fields(registration.app_id).unwrap();
        Ok(
            vdf::string(fields, "exe") == Some(executable_field(&launch.executable))
                && vdf::string(fields, "StartDir").is_some_and(|value| {
                    value == launch.working_directory
                        || value == executable_field(&launch.working_directory)
                })
                && vdf::string(fields, "LaunchOptions") == Some(launch_options(&launch.arguments)),
        )
    }
    pub fn upsert(
        &mut self,
        registration: &Registration,
        name: &str,
        previous_name: Option<&str>,
    ) -> Result<()> {
        for value in [
            &registration.exported.executable,
            &registration.exported.working_directory,
            name,
        ]
        .into_iter()
        .chain(registration.exported.arguments.iter().map(String::as_str))
        {
            if value.contains('\0') {
                return Err(Error::other("Shortcut fields cannot contain NUL"));
            }
        }
        let exists = self.verify(registration)?;
        if !exists {
            let items = self.items_mut();
            let key = (0..)
                .map(|i| i.to_string())
                .find(|key| !items.iter().any(|e| e.key == key.as_bytes()))
                .unwrap();
            items.push(Entry::new(
                &key,
                Value::Object(vec![Entry::new("appid", vdf::int(registration.app_id))]),
            ));
        }
        let fields = self
            .items_mut()
            .iter_mut()
            .find_map(|entry| match &mut entry.value {
                Value::Object(fields)
                    if vdf::number(fields, "appid") == Some(registration.app_id) =>
                {
                    Some(fields)
                }
                _ => None,
            })
            .unwrap();
        // Preserve a Steam-edited name unless the user explicitly changes it in Drop.
        if !exists
            || previous_name.is_none()
            || previous_name != Some(name)
            || vdf::string(fields, "AppName").as_deref() == previous_name
        {
            vdf::set(fields, "AppName", vdf::text(name));
        }
        vdf::set(
            fields,
            "exe",
            vdf::text(&executable_field(&registration.exported.executable)),
        );
        vdf::set(
            fields,
            "StartDir",
            vdf::text(&executable_field(&registration.exported.working_directory)),
        );
        vdf::set(
            fields,
            "LaunchOptions",
            vdf::text(&launch_options(&registration.exported.arguments)),
        );
        if !exists {
            vdf::set(fields, "icon", vdf::text(&registration.exported.executable));
            for (key, value) in [
                ("IsHidden", 0),
                ("AllowDesktopConfig", 1),
                ("AllowOverlay", 1),
                ("OpenVR", 0),
            ] {
                vdf::set(fields, key, vdf::int(value));
            }
            vdf::set(fields, "ShortcutPath", vdf::text(""));
        }
        let mut tags = match vdf::get(fields, "tags") {
            Some(Value::Object(tags)) => tags.clone(),
            None => vec![],
            _ => return Err(Error::other("Invalid Steam tags")),
        };
        for tag in [
            "Drop".to_string(),
            format!("Drop:{}", registration.ownership_token),
        ] {
            if !tags.iter().any(|e| e.value == vdf::text(&tag)) {
                let key = (0..)
                    .map(|i| i.to_string())
                    .find(|key| !tags.iter().any(|e| e.key == key.as_bytes()))
                    .unwrap();
                tags.push(Entry::new(&key, vdf::text(&tag)));
            }
        }
        vdf::set(fields, "tags", Value::Object(tags));
        Ok(())
    }
    pub fn remove(&mut self, registration: &Registration) -> Result<()> {
        if self.verify(registration)? {
            self.items_mut().retain(|entry| !matches!(&entry.value,Value::Object(fields) if vdf::number(fields,"appid") == Some(registration.app_id)));
            for (index, entry) in self.items_mut().iter_mut().enumerate() {
                entry.key = index.to_string().into_bytes();
            }
        }
        Ok(())
    }
    pub fn bytes(&self) -> Vec<u8> {
        vdf::encode(&self.entries)
    }
}
/// A sibling advisory lock serializes Drop processes, not Steam itself. Check
/// Steam again and re-read the source immediately before the atomic replacement.
pub fn mutate<T>(
    path: &Path,
    mut guard: impl FnMut() -> Result<()>,
    change: impl FnOnce(&mut Shortcuts) -> Result<T>,
) -> Result<T> {
    guard()?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::other("Missing shortcut parent"))?;
    fs::create_dir_all(parent)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(".drop-shortcuts.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| Error::other("Another Drop shortcut operation is in progress"))?;
    let source = read_source(path)?;
    let mut shortcuts = Shortcuts::parse(source.as_deref())?;
    let result = change(&mut shortcuts)?;
    let bytes = shortcuts.bytes();
    if source.as_deref() == Some(bytes.as_slice())
        || (source.is_none() && bytes == Shortcuts::parse(None)?.bytes())
    {
        guard()?;
        if read_source(path)? != source {
            return Err(Error::other(
                "Steam shortcuts changed during the operation. Retry.",
            ));
        }
        return Ok(result);
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    if path.exists() {
        temporary
            .as_file()
            .set_permissions(fs::metadata(path)?.permissions())?;
    }
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    if let Some(original) = &source {
        let backup = parent.join(format!("shortcuts.vdf.drop-{}.bak", uuid::Uuid::new_v4()));
        let mut backup = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(backup)?;
        backup.write_all(original)?;
        backup.sync_all()?;
    }
    guard()?;
    if read_source(path)? != source {
        return Err(Error::other(
            "Steam shortcuts changed during the operation. Retry; no changes were written.",
        ));
    }
    temporary.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(result)
}
