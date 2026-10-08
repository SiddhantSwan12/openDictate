use std::path::PathBuf;

/// %APPDATA%\OpenDictate: workspace.json, Models\ and Meetings\.
pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("OpenDictate")
}

pub fn models_dir() -> PathBuf {
    app_dir().join("Models")
}

pub fn meetings_dir() -> PathBuf {
    app_dir().join("Meetings")
}
