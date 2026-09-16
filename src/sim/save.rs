//! セーブ・ロード（design.md §14 / FR-SAVE-*）
//!
//! 形式は RON。`defs` はセーブに含めず、ロード時に現在のデータファイルを再適用する
//! （バランス調整のたびにセーブが壊れるのを避けるため: design.md §4.1）。
//! `format_version` の不一致は**拒否**する。マイグレーションは v1 では書かない。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::defs::Defs;
use super::Game;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    pub format_version: u32,
    pub game: Game,
}

#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    Encode(String),
    Decode(String),
    /// 非互換データ。破壊せず明示的に拒否する（FR-SAVE-03）。
    Incompatible { found: u32, expected: u32 },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "io: {e}"),
            SaveError::Encode(e) => write!(f, "encode: {e}"),
            SaveError::Decode(e) => write!(f, "decode: {e}"),
            SaveError::Incompatible { found, expected } => {
                write!(f, "incompatible save: found {found}, expected {expected}")
            }
        }
    }
}

impl std::error::Error for SaveError {}

pub fn to_string(game: &Game) -> Result<String, SaveError> {
    let file = SaveFile { format_version: FORMAT_VERSION, game: game.clone() };
    ron::ser::to_string(&file).map_err(|e| SaveError::Encode(e.to_string()))
}

pub fn from_string(src: &str, defs: Arc<Defs>) -> Result<Game, SaveError> {
    // まず版番号だけを見る。非互換ならデコードを試みない。
    let file: SaveFile = ron::from_str(src).map_err(|e| SaveError::Decode(e.to_string()))?;
    if file.format_version != FORMAT_VERSION {
        return Err(SaveError::Incompatible {
            found: file.format_version,
            expected: FORMAT_VERSION,
        });
    }
    let mut game = file.game;
    game.defs = defs;
    game.revalidate_after_load();
    Ok(game)
}

pub fn write_to(path: &Path, game: &Game) -> Result<(), SaveError> {
    let s = to_string(game)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(SaveError::Io)?;
    }
    std::fs::write(path, s).map_err(SaveError::Io)
}

pub fn read_from(path: &Path, defs: Arc<Defs>) -> Result<Game, SaveError> {
    let s = std::fs::read_to_string(path).map_err(SaveError::Io)?;
    from_string(&s, defs)
}

/// 保存先。`dirs` クレートは使わず環境変数を直接読む（NFR-06: 依存を増やさない）。
pub fn save_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("kabaddism").join("saves");
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("kabaddism").join("saves");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share/kabaddism/saves");
    }
    PathBuf::from("saves")
}

pub fn slot_path(slot: &str) -> PathBuf {
    save_dir().join(format!("{slot}.ron"))
}

/// オートセーブのローテーション3枠（design.md §14）。
pub fn autosave_slot(index: u32) -> String {
    format!("auto{}", index % 3)
}
