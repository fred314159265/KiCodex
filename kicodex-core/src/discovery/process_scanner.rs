use std::path::PathBuf;

use sysinfo::{ProcessRefreshKind, System, UpdateKind};

/// KiCad project-related file extensions whose parent directory is the project root.
const KICAD_PROJECT_EXTENSIONS: &[&str] =
    &[".kicad_pro", ".kicad_sch", ".kicad_pcb", ".kicad_dru"];

/// Scan running processes for KiCad instances and extract project root directories.
///
/// Detects project directories from any KiCad-related process argument ending in a
/// recognised KiCad file extension (`.kicad_pro`, `.kicad_sch`, `.kicad_pcb`, …).
/// This covers the common case where KiCad's launcher has no argument but eeschema
/// or pcbnew was opened with a file path.
///
/// Returns a deduplicated, sorted list of project root directories.
pub fn scan_kicad_processes() -> Vec<PathBuf> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );

    let mut project_dirs: Vec<PathBuf> = Vec::new();

    for process in system.processes().values() {
        let name = process.name().to_string_lossy().to_lowercase();
        if !name.contains("kicad") && !name.contains("eeschema") && !name.contains("pcbnew") {
            continue;
        }

        let cmd = process.cmd();
        tracing::debug!(
            "Found KiCad process: name={}, args={:?}",
            name,
            cmd.iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect::<Vec<_>>()
        );

        for arg in cmd {
            let arg_str = arg.to_string_lossy();
            let matches = KICAD_PROJECT_EXTENSIONS
                .iter()
                .any(|ext| arg_str.ends_with(*ext));
            if matches {
                let path = PathBuf::from(arg_str.as_ref());
                if let Some(parent) = path.parent() {
                    let dir = parent.to_path_buf();
                    if !project_dirs.contains(&dir) {
                        project_dirs.push(dir);
                    }
                }
            }
        }
    }

    project_dirs.sort();
    project_dirs
}

/// Extract project directories from a list of command-line argument vectors.
/// This is the testable core logic, independent of sysinfo.
pub fn extract_project_dirs_from_args(args_lists: &[Vec<String>]) -> Vec<PathBuf> {
    let mut project_dirs: Vec<PathBuf> = Vec::new();

    for args in args_lists {
        for arg in args {
            let matches = KICAD_PROJECT_EXTENSIONS
                .iter()
                .any(|ext| arg.ends_with(*ext));
            if matches {
                let path = PathBuf::from(arg);
                if let Some(parent) = path.parent() {
                    let dir = parent.to_path_buf();
                    if !project_dirs.contains(&dir) {
                        project_dirs.push(dir);
                    }
                }
            }
        }
    }

    project_dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_project_dirs_from_args() {
        let args = vec![
            vec![
                "kicad".to_string(),
                "/home/user/project1/my_board.kicad_pro".to_string(),
            ],
            vec![
                "kicad".to_string(),
                "/home/user/project2/another.kicad_pro".to_string(),
            ],
        ];
        let dirs = extract_project_dirs_from_args(&args);
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0], PathBuf::from("/home/user/project1"));
        assert_eq!(dirs[1], PathBuf::from("/home/user/project2"));
    }

    #[test]
    fn test_extract_deduplicates() {
        let args = vec![
            vec!["kicad".to_string(), "/project/board.kicad_pro".to_string()],
            vec!["kicad".to_string(), "/project/board.kicad_pro".to_string()],
        ];
        let dirs = extract_project_dirs_from_args(&args);
        assert_eq!(dirs.len(), 1);
    }

    #[test]
    fn test_extract_kicad_sch_and_pcb() {
        // .kicad_sch and .kicad_pcb also map to the project directory.
        let args = vec![
            vec![
                "eeschema".to_string(),
                "/project/schematic.kicad_sch".to_string(),
            ],
            vec![
                "pcbnew".to_string(),
                "/project/board.kicad_pcb".to_string(),
            ],
        ];
        let dirs = extract_project_dirs_from_args(&args);
        // Both resolve to the same parent directory — should be deduped.
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0], PathBuf::from("/project"));
    }

    #[test]
    fn test_extract_ignores_non_kicad_files() {
        let args = vec![vec![
            "kicad".to_string(),
            "/project/readme.txt".to_string(),
        ]];
        let dirs = extract_project_dirs_from_args(&args);
        assert!(dirs.is_empty());
    }

    #[test]
    fn test_scan_does_not_crash() {
        // Just verify it doesn't panic — no KiCad running in CI
        let _ = scan_kicad_processes();
    }
}
