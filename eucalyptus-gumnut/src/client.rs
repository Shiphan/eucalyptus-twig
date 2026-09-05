use std::{io::Write, os::unix::net::UnixStream};

use crate::item::ItemKind;

pub fn show(item: ItemKind) -> Result<(), std::io::Error> {
    let mut stream = UnixStream::connect(crate::daemon::SOCKET_PATH.as_path())?;
    let bytes: &[u8] = match item {
        ItemKind::Backlight => b"show/backlight",
        ItemKind::Media => b"show/media",
        ItemKind::PowerProfile => b"show/power_profile",
        ItemKind::Volume => b"show/volume",
    };
    stream.write_all(bytes)
}

pub fn stop() -> Result<(), std::io::Error> {
    let mut stream = UnixStream::connect(crate::daemon::SOCKET_PATH.as_path())?;
    stream.write_all(b"stop")
}
