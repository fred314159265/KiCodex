//! KiCad library table parser and symbol/footprint resolver.
//!
//! Parses `sym-lib-table` and `fp-lib-table` files (both global and project-local),
//! resolves library URIs, and checks that referenced symbols/footprints exist.
//! Library contents are loaded lazily on first lookup to avoid reading hundreds
//! of files when only a few libraries are actually referenced.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Result of looking up a symbol or footprint reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibLookup {
    /// The reference was found.
    Found,
    /// The library name was not found in any table.
    LibraryNotFound(String),
    /// The library exists but the specific entry doesn't.
    EntryNotFound(String, String),
    /// The library URI couldn't be resolved or read.
    LibraryUnreadable(String),
}

/// Parsed library table entry.
#[derive(Debug, Clone)]
struct LibEntry {
    name: String,
    uri: String,
}

/// Lazily-loaded library content.
enum LibContent {
    /// URI resolved but not yet loaded.
    Pending(PathBuf),
    /// Loaded successfully — Vec for ordered listing, HashSet for O(1) lookup.
    Loaded(Vec<String>, HashSet<String>),
    /// Failed to load or resolve.
    Unreadable,
}

/// Holds KiCad symbol and footprint library table info for validation lookups.
/// Library contents are loaded lazily on first access.
pub struct KicadLibraries {
    symbol_libs: Mutex<HashMap<String, LibContent>>,
    footprint_libs: Mutex<HashMap<String, LibContent>>,
}

impl KicadLibraries {
    /// Load lib table entries from global + optional project-local lib tables.
    /// Does NOT read the actual library files — that happens lazily on lookup.
    pub fn load(project_dir: Option<&Path>) -> Result<Self, String> {
        let mut symbol_entries: Vec<LibEntry> = Vec::new();
        let mut footprint_entries: Vec<LibEntry> = Vec::new();

        // Load global lib tables
        for path in find_global_lib_tables("sym-lib-table") {
            if let Ok(entries) = parse_lib_table_file(&path) {
                symbol_entries.extend(entries);
            }
        }
        for path in find_global_lib_tables("fp-lib-table") {
            if let Ok(entries) = parse_lib_table_file(&path) {
                footprint_entries.extend(entries);
            }
        }

        // Load project-local lib tables
        if let Some(proj) = project_dir {
            let sym_path = proj.join("sym-lib-table");
            if sym_path.exists() {
                if let Ok(entries) = parse_lib_table_file(&sym_path) {
                    symbol_entries.extend(entries);
                }
            }
            let fp_path = proj.join("fp-lib-table");
            if fp_path.exists() {
                if let Ok(entries) = parse_lib_table_file(&fp_path) {
                    footprint_entries.extend(entries);
                }
            }
        }

        // Resolve URIs but don't load contents yet
        let symbol_libs = build_lib_map(&symbol_entries);
        let footprint_libs = build_lib_map(&footprint_entries);

        Ok(KicadLibraries {
            symbol_libs: Mutex::new(symbol_libs),
            footprint_libs: Mutex::new(footprint_libs),
        })
    }

    /// Check if a symbol reference like "Device:R" exists.
    pub fn has_symbol(&self, reference: &str) -> LibLookup {
        lazy_lookup(&self.symbol_libs, reference, load_symbol_lib)
    }

    /// Check if a footprint reference like "Resistor_SMD:R_0603" exists.
    pub fn has_footprint(&self, reference: &str) -> LibLookup {
        lazy_lookup(&self.footprint_libs, reference, load_footprint_lib)
    }

    /// List all symbol library names.
    pub fn list_symbol_libraries(&self) -> Vec<String> {
        list_lib_names(&self.symbol_libs)
    }

    /// List all footprint library names.
    pub fn list_footprint_libraries(&self) -> Vec<String> {
        list_lib_names(&self.footprint_libs)
    }

    /// List symbol entries in a library, forcing lazy load if needed.
    pub fn list_symbols(&self, lib_name: &str) -> Option<Vec<String>> {
        list_entries(&self.symbol_libs, lib_name, load_symbol_lib)
    }

    /// List footprint entries in a library, forcing lazy load if needed.
    pub fn list_footprints(&self, lib_name: &str) -> Option<Vec<String>> {
        list_entries(&self.footprint_libs, lib_name, load_footprint_lib)
    }
}

fn build_lib_map(entries: &[LibEntry]) -> HashMap<String, LibContent> {
    let mut map = HashMap::new();
    for entry in entries {
        if map.contains_key(&entry.name) {
            continue;
        }
        let resolved = resolve_env_vars(&entry.uri);
        let path = PathBuf::from(&resolved);
        if path.exists() {
            map.insert(entry.name.clone(), LibContent::Pending(path));
        } else {
            map.insert(entry.name.clone(), LibContent::Unreadable);
        }
    }
    map
}

fn list_lib_names(libs: &Mutex<HashMap<String, LibContent>>) -> Vec<String> {
    let map = libs.lock().unwrap();
    let mut names: Vec<String> = map.keys().cloned().collect();
    names.sort();
    names
}

/// Force-load a pending library into the cache and return its entries, or None on failure.
fn force_load_pending(
    map: &mut HashMap<String, LibContent>,
    lib_name: &str,
    loader: fn(&Path) -> Option<Vec<String>>,
) -> Option<Vec<String>> {
    let LibContent::Pending(path) = map.remove(lib_name).unwrap() else {
        unreachable!()
    };
    match loader(&path) {
        Some(entries) => {
            let set: HashSet<String> = entries.iter().cloned().collect();
            map.insert(lib_name.to_string(), LibContent::Loaded(entries.clone(), set));
            Some(entries)
        }
        None => {
            map.insert(lib_name.to_string(), LibContent::Unreadable);
            None
        }
    }
}

/// Perform a lazy lookup: load the library on first access, then check for the entry.
fn lazy_lookup(
    libs: &Mutex<HashMap<String, LibContent>>,
    reference: &str,
    loader: fn(&Path) -> Option<Vec<String>>,
) -> LibLookup {
    let Some((lib_name, entry_name)) = reference.split_once(':') else {
        return LibLookup::LibraryNotFound(reference.to_string());
    };

    let mut map = libs.lock().unwrap();
    match map.get(lib_name) {
        None => LibLookup::LibraryNotFound(lib_name.to_string()),
        Some(LibContent::Loaded(_, set)) => {
            if set.contains(entry_name) {
                LibLookup::Found
            } else {
                LibLookup::EntryNotFound(lib_name.to_string(), entry_name.to_string())
            }
        }
        Some(LibContent::Unreadable) => LibLookup::LibraryUnreadable(lib_name.to_string()),
        Some(LibContent::Pending(_)) => {
            match force_load_pending(&mut map, lib_name, loader) {
                Some(entries) => {
                    let set: HashSet<String> = entries.into_iter().collect();
                    if set.contains(entry_name) {
                        LibLookup::Found
                    } else {
                        LibLookup::EntryNotFound(lib_name.to_string(), entry_name.to_string())
                    }
                }
                None => LibLookup::LibraryUnreadable(lib_name.to_string()),
            }
        }
    }
}

/// List all entries in a library, forcing lazy load if needed.
fn list_entries(
    libs: &Mutex<HashMap<String, LibContent>>,
    lib_name: &str,
    loader: fn(&Path) -> Option<Vec<String>>,
) -> Option<Vec<String>> {
    let mut map = libs.lock().unwrap();
    match map.get(lib_name) {
        None => None,
        Some(LibContent::Loaded(entries, _)) => Some(entries.clone()),
        Some(LibContent::Unreadable) => None,
        Some(LibContent::Pending(_)) => force_load_pending(&mut map, lib_name, loader),
    }
}

fn load_symbol_lib(path: &Path) -> Option<Vec<String>> {
    let content = std::fs::read_to_string(path).ok()?;
    Some(extract_symbol_names(&content))
}

fn load_footprint_lib(path: &Path) -> Option<Vec<String>> {
    if path.is_dir() {
        extract_footprints_from_dir(path).ok()
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// S-expression parsing for lib tables
// ---------------------------------------------------------------------------

/// Parse a `sym-lib-table` or `fp-lib-table` file.
fn parse_lib_table_file(path: &Path) -> Result<Vec<LibEntry>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    parse_lib_table(&content)
}

/// Parse the content of a lib table file and extract entries.
fn parse_lib_table(content: &str) -> Result<Vec<LibEntry>, String> {
    let mut entries = Vec::new();

    // Find each (lib ...) block and extract name and uri
    let mut pos = 0;
    let bytes = content.as_bytes();
    while pos < bytes.len() {
        if let Some(idx) = find_token(content, pos, "(lib ") {
            let block_start = idx;
            if let Some(block_end) = find_matching_paren(content, block_start) {
                let block = &content[block_start..=block_end];
                let name = extract_field(block, "name");
                let uri = extract_field(block, "uri");
                if let (Some(name), Some(uri)) = (name, uri) {
                    entries.push(LibEntry { name, uri });
                }
                pos = block_end + 1;
            } else {
                pos = idx + 1;
            }
        } else {
            break;
        }
    }

    Ok(entries)
}

/// Find a token in the string starting from `start`.
fn find_token(s: &str, start: usize, token: &str) -> Option<usize> {
    s[start..].find(token).map(|i| i + start)
}

/// Find the index of the closing parenthesis matching the opening one at `open`.
fn find_matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if escape {
            escape = false;
            continue;
        }
        match b {
            b'\\' if in_quote => escape = true,
            b'"' => in_quote = !in_quote,
            b'(' if !in_quote => depth += 1,
            b')' if !in_quote => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the value of a field like `(name "Value")` or `(name Value)` from an S-expression block.
fn extract_field(block: &str, field: &str) -> Option<String> {
    // Try quoted form first: (field "value")
    let quoted_pattern = format!("({} \"", field);
    if let Some(start) = block.find(&quoted_pattern) {
        let value_start = start + quoted_pattern.len();
        let rest = &block[value_start..];
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }

    // Try unquoted form: (field value)
    let unquoted_pattern = format!("({} ", field);
    if let Some(start) = block.find(&unquoted_pattern) {
        let value_start = start + unquoted_pattern.len();
        let rest = &block[value_start..];
        let end = rest.find(')')?;
        return Some(rest[..end].to_string());
    }

    None
}

// ---------------------------------------------------------------------------
// Symbol extraction from .kicad_sym files
// ---------------------------------------------------------------------------

/// Extract symbol names from `.kicad_sym` file content.
///
/// Looks for top-level `(symbol "Name" ...)` entries (depth 1 inside
/// `kicad_symbol_lib`). Strips library prefix if present.
///
/// Uses a single-pass scan with incremental depth tracking (O(n)).
fn extract_symbol_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let token = b"(symbol \"";
    let token_len = token.len();

    let mut depth: i32 = 0;
    let mut in_quote = false;
    let mut escape = false;
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        if escape {
            escape = false;
            i += 1;
            continue;
        }

        if b == b'\\' && in_quote {
            escape = true;
            i += 1;
            continue;
        }

        if b == b'"' {
            in_quote = !in_quote;
            i += 1;
            continue;
        }

        if in_quote {
            i += 1;
            continue;
        }

        if b == b'(' {
            // Check if this is `(symbol "` at depth 1 (about to become depth 1 after this open paren)
            if depth == 1 && i + token_len <= len && &bytes[i..i + token_len] == token {
                // Extract the name between the quotes
                let name_start = i + token_len;
                if let Some(rel_end) = bytes[name_start..].iter().position(|&c| c == b'"') {
                    let raw_name = &content[name_start..name_start + rel_end];
                    // Strip library prefix (e.g., "Device:R" -> "R")
                    let name = match raw_name.split_once(':') {
                        Some((_, after)) => after.to_string(),
                        None => raw_name.to_string(),
                    };
                    names.push(name);
                    // Skip past the closing quote
                    i = name_start + rel_end + 1;
                    depth += 1;
                    continue;
                }
            }
            depth += 1;
            i += 1;
            continue;
        }

        if b == b')' {
            depth -= 1;
            i += 1;
            continue;
        }

        i += 1;
    }

    names
}

// ---------------------------------------------------------------------------
// Footprint extraction from .pretty directories
// ---------------------------------------------------------------------------

/// Extract footprint names from a `.pretty` directory.
fn extract_footprints_from_dir(path: &Path) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let entries =
        std::fs::read_dir(path).map_err(|e| format!("read_dir {}: {}", path.display(), e))?;
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let fname = fname.to_string_lossy();
        if let Some(stem) = fname.strip_suffix(".kicad_mod") {
            names.push(stem.to_string());
        }
    }
    Ok(names)
}

// ---------------------------------------------------------------------------
// Environment variable resolution
// ---------------------------------------------------------------------------

/// Expand `${VAR}` patterns in a URI string.
fn resolve_env_vars(uri: &str) -> String {
    let mut result = String::with_capacity(uri.len());
    let mut chars = uri.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
                var_name.push(c);
            }
            if let Ok(val) = std::env::var(&var_name) {
                result.push_str(&val);
            } else if let Some(fallback) = kicad_default_path(&var_name) {
                result.push_str(&fallback);
            } else {
                // Leave unresolved
                result.push_str(&format!("${{{}}}", var_name));
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Provide fallback paths for common KiCad environment variables.
fn kicad_default_path(var_name: &str) -> Option<String> {
    let symbol_vars = [
        "KICAD8_SYMBOL_DIR",
        "KICAD9_SYMBOL_DIR",
        "KICAD7_SYMBOL_DIR",
        "KICAD_SYMBOL_DIR",
    ];
    let footprint_vars = [
        "KICAD8_FOOTPRINT_DIR",
        "KICAD9_FOOTPRINT_DIR",
        "KICAD7_FOOTPRINT_DIR",
        "KICAD_FOOTPRINT_DIR",
    ];

    let is_symbol = symbol_vars.contains(&var_name);
    let is_footprint = footprint_vars.contains(&var_name);

    if !is_symbol && !is_footprint {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        let base_dirs = [
            r"C:\Program Files\KiCad\9.0\share\kicad",
            r"C:\Program Files\KiCad\8.0\share\kicad",
            r"C:\Program Files\KiCad\share\kicad",
        ];
        let subdir = if is_symbol { "symbols" } else { "footprints" };
        for base in &base_dirs {
            let candidate = format!("{}\\{}", base, subdir);
            if Path::new(&candidate).exists() {
                return Some(candidate);
            }
        }
        Some(format!(
            r"C:\Program Files\KiCad\8.0\share\kicad\{}",
            subdir
        ))
    }

    #[cfg(target_os = "macos")]
    {
        let subdir = if is_symbol { "symbols" } else { "footprints" };
        Some(format!(
            "/Applications/KiCad/KiCad.app/Contents/SharedSupport/{}",
            subdir
        ))
    }

    #[cfg(target_os = "linux")]
    {
        let subdir = if is_symbol { "symbols" } else { "footprints" };
        Some(format!("/usr/share/kicad/{}", subdir))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Global config path discovery
// ---------------------------------------------------------------------------

/// Find global lib table files across KiCad version directories.
fn find_global_lib_tables(filename: &str) -> Vec<PathBuf> {
    let mut results = Vec::new();

    let config_base = global_kicad_config_dir();
    let Some(config_base) = config_base else {
        return results;
    };

    // Try versioned subdirectories (newest first)
    let version_dirs = ["9.0", "8.0", "7.0", "6.0"];
    for ver in &version_dirs {
        let path = config_base.join(ver).join(filename);
        if path.exists() {
            results.push(path);
            break; // Use only the latest version found
        }
    }

    // Also check the base config dir itself
    let path = config_base.join(filename);
    if path.exists() && !results.iter().any(|p| p == &path) {
        results.push(path);
    }

    results
}

/// Get the KiCad configuration directory for the current OS.
fn global_kicad_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|appdata| PathBuf::from(appdata).join("kicad"))
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|home| home.join("Library/Preferences/kicad"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::home_dir().map(|home| home.join(".config/kicad"))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lib_table() {
        let content = r#"(sym_lib_table
  (version 7)
  (lib (name "Device")(type "KiCad")(uri "${KICAD8_SYMBOL_DIR}/Device.kicad_sym")(options "")(descr ""))
  (lib (name "Connector")(type "KiCad")(uri "${KICAD8_SYMBOL_DIR}/Connector.kicad_sym")(options "")(descr ""))
)"#;
        let entries = parse_lib_table(content).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Device");
        assert_eq!(entries[0].uri, "${KICAD8_SYMBOL_DIR}/Device.kicad_sym");
        assert_eq!(entries[1].name, "Connector");
    }

    #[test]
    fn test_parse_lib_table_fp() {
        let content = r#"(fp_lib_table
  (version 7)
  (lib (name "Resistor_SMD")(type "KiCad")(uri "${KICAD8_FOOTPRINT_DIR}/Resistor_SMD.pretty")(options "")(descr ""))
)"#;
        let entries = parse_lib_table(content).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Resistor_SMD");
    }

    #[test]
    fn test_extract_symbol_names() {
        let content = r#"(kicad_symbol_lib
  (version 20231120)
  (symbol "Device:R"
    (property "Reference" "R")
    (symbol "Device:R_0_1"
      (polyline (pts (xy 0 0)) )
    )
  )
  (symbol "Device:C"
    (property "Reference" "C")
  )
)"#;
        let names = extract_symbol_names(content);
        assert_eq!(names, vec!["R", "C"]);
    }

    #[test]
    fn test_extract_symbol_names_no_prefix() {
        let content = r#"(kicad_symbol_lib
  (symbol "MyPart"
    (property "Reference" "U")
  )
)"#;
        let names = extract_symbol_names(content);
        assert_eq!(names, vec!["MyPart"]);
    }

    #[test]
    fn test_extract_footprints_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("R_0603_1608Metric.kicad_mod"), "").unwrap();
        std::fs::write(dir.path().join("R_0402_1005Metric.kicad_mod"), "").unwrap();
        std::fs::write(dir.path().join("README.md"), "").unwrap();

        let mut names = extract_footprints_from_dir(dir.path()).unwrap();
        names.sort();
        assert_eq!(names, vec!["R_0402_1005Metric", "R_0603_1608Metric"]);
    }

    #[test]
    fn test_resolve_env_vars() {
        std::env::set_var("TEST_KICAD_VAR", "/test/path");
        let result = resolve_env_vars("${TEST_KICAD_VAR}/Device.kicad_sym");
        assert_eq!(result, "/test/path/Device.kicad_sym");
        std::env::remove_var("TEST_KICAD_VAR");
    }

    #[test]
    fn test_resolve_env_vars_no_vars() {
        let result = resolve_env_vars("/some/literal/path.kicad_sym");
        assert_eq!(result, "/some/literal/path.kicad_sym");
    }

    #[test]
    fn test_lookup_found() {
        let mut libs = HashMap::new();
        let entries = vec!["R".to_string(), "C".to_string()];
        let set: HashSet<String> = entries.iter().cloned().collect();
        libs.insert(
            "Device".to_string(),
            LibContent::Loaded(entries, set),
        );
        let libs = Mutex::new(libs);

        assert_eq!(lazy_lookup(&libs, "Device:R", |_| None), LibLookup::Found);
    }

    #[test]
    fn test_lookup_entry_not_found() {
        let mut libs = HashMap::new();
        let entries = vec!["R".to_string()];
        let set: HashSet<String> = entries.iter().cloned().collect();
        libs.insert(
            "Device".to_string(),
            LibContent::Loaded(entries, set),
        );
        let libs = Mutex::new(libs);

        assert_eq!(
            lazy_lookup(&libs, "Device:L", |_| None),
            LibLookup::EntryNotFound("Device".to_string(), "L".to_string())
        );
    }

    #[test]
    fn test_lookup_library_not_found() {
        let libs: Mutex<HashMap<String, LibContent>> = Mutex::new(HashMap::new());

        assert_eq!(
            lazy_lookup(&libs, "Missing:R", |_| None),
            LibLookup::LibraryNotFound("Missing".to_string())
        );
    }

    #[test]
    fn test_lookup_library_unreadable() {
        let mut libs = HashMap::new();
        libs.insert("Broken".to_string(), LibContent::Unreadable);
        let libs = Mutex::new(libs);

        assert_eq!(
            lazy_lookup(&libs, "Broken:X", |_| None),
            LibLookup::LibraryUnreadable("Broken".to_string())
        );
    }

    #[test]
    fn test_find_matching_paren() {
        let s = "(lib (name \"Device\")(uri \"path\"))";
        assert_eq!(find_matching_paren(s, 0), Some(s.len() - 1));
    }

    #[test]
    fn test_extract_field_quoted() {
        let block = r#"(lib (name "Device")(type "KiCad")(uri "/path/to/lib.kicad_sym"))"#;
        assert_eq!(extract_field(block, "name"), Some("Device".to_string()));
        assert_eq!(
            extract_field(block, "uri"),
            Some("/path/to/lib.kicad_sym".to_string())
        );
        assert_eq!(extract_field(block, "missing"), None);
    }

    #[test]
    fn test_extract_field_unquoted() {
        let block = r#"(lib (name Audio_Module)(type Kicad)(uri ${KICAD9_FOOTPRINT_DIR}/Audio_Module.pretty)(options "")(descr ""))"#;
        assert_eq!(
            extract_field(block, "name"),
            Some("Audio_Module".to_string())
        );
        assert_eq!(
            extract_field(block, "uri"),
            Some("${KICAD9_FOOTPRINT_DIR}/Audio_Module.pretty".to_string())
        );
    }
}
