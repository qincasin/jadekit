use std::io;
use std::path::PathBuf;

pub const APP_DATA_DIR_NAME: &str = ".jadekit";

fn home_dir() -> Result<PathBuf, io::Error> {
    dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home directory not found"))
}

pub fn data_dir() -> Result<PathBuf, io::Error> {
    Ok(home_dir()?.join(APP_DATA_DIR_NAME))
}

pub fn data_file(name: &str) -> Result<PathBuf, io::Error> {
    Ok(data_dir()?.join(name))
}

pub fn data_subdir(name: &str) -> Result<PathBuf, io::Error> {
    Ok(data_dir()?.join(name))
}
