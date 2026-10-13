//! Black Ops II (T6) sound banks, read in place from the player's own install.
//!
//! A `.sabl` (loaded) or `.sabs` (streamed) bank starts with a 2048-byte header (magic `2UX#`,
//! version 14) and holds a table of 20-byte entries. Each entry's id is `SND_HashName` of the
//! sound's file name, as written in the alias tables of the zombie fastfiles.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const MAGIC: u32 = u32::from_le_bytes(*b"2UX#");
pub const VERSION: u32 = 14;
pub const HEADER_SIZE: usize = 2048;
pub const ENTRY_SIZE: usize = 20;

pub const FORMAT_PCMS16: u8 = 0;
pub const FORMAT_FLAC: u8 = 8;

/// T6 frame rates by `frameRateIndex`.
pub const FRAME_RATES: [u32; 9] = [8000, 12000, 16000, 24000, 32000, 44100, 48000, 96000, 192000];

/// The game's own name hash (sdbm-like, lowercase, never 0).
pub fn snd_hash_name(name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }
    let mut h: u32 = 0x1505;
    for b in name.bytes() {
        h = (b.to_ascii_lowercase() as u32).wrapping_add(h.wrapping_mul(0x1003F));
    }
    if h == 0 { 1 } else { h }
}

/// A FileSource as OpenAssetTools writes it, back to the name the game hashes.
pub fn file_source_to_asset_name(file_source: &str) -> &str {
    let s = file_source.trim();
    for ext in [".flac", ".wav"] {
        if s.len() > ext.len() && s[s.len() - ext.len()..].eq_ignore_ascii_case(ext) {
            return &s[..s.len() - ext.len()];
        }
    }
    s
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BankEntry {
    pub id: u32,
    pub size: u32,
    pub offset: u32,
    pub frame_count: u32,
    pub frame_rate_index: u8,
    pub channel_count: u8,
    pub looping: u8,
    pub format: u8,
}

#[derive(Debug)]
pub struct BankHeader {
    pub entry_count: u32,
    pub file_size: u64,
    pub entry_offset: u64,
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

pub fn parse_header(b: &[u8]) -> Result<BankHeader, String> {
    if b.len() < HEADER_SIZE {
        return Err("short header".into());
    }
    let magic = u32_at(b, 0);
    if magic != MAGIC {
        return Err(format!("bad magic {magic:#x}"));
    }
    let version = u32_at(b, 4);
    if version != VERSION {
        return Err(format!("unsupported version {version}"));
    }
    let entry_size = u32_at(b, 8) as usize;
    if entry_size != ENTRY_SIZE {
        return Err(format!("unexpected entry size {entry_size}"));
    }
    Ok(BankHeader { entry_count: u32_at(b, 20), file_size: u64_at(b, 32), entry_offset: u64_at(b, 40) })
}

pub fn parse_entry(b: &[u8]) -> BankEntry {
    BankEntry {
        id: u32_at(b, 0),
        size: u32_at(b, 4),
        offset: u32_at(b, 8),
        frame_count: u32_at(b, 12),
        frame_rate_index: b[16],
        channel_count: b[17],
        looping: b[18],
        format: b[19],
    }
}

/// Reads one bank's entry table (not its audio).
pub fn read_bank_entries(path: &Path) -> Result<Vec<BankEntry>, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let real_size = f.metadata().map_err(|e| e.to_string())?.len();
    let mut head = vec![0u8; HEADER_SIZE];
    f.read_exact(&mut head).map_err(|e| e.to_string())?;
    let h = parse_header(&head)?;
    if h.file_size != real_size {
        return Err(format!("header says {} bytes, file has {real_size}", h.file_size));
    }
    let table_len = h.entry_count as u64 * ENTRY_SIZE as u64;
    if h.entry_offset == 0 || h.entry_offset + table_len > real_size {
        return Err("entry table outside file".into());
    }
    f.seek(SeekFrom::Start(h.entry_offset)).map_err(|e| e.to_string())?;
    let mut table = vec![0u8; table_len as usize];
    f.read_exact(&mut table).map_err(|e| e.to_string())?;
    Ok(table
        .chunks_exact(ENTRY_SIZE)
        .map(parse_entry)
        .filter(|e| e.offset != 0 && (e.offset as u64 + e.size as u64) <= real_size)
        .collect())
}

/// Where one BO2 sound lives inside the player's own sound banks.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SoundRef {
    /// Bank file, relative to the BO2 folder.
    pub bank: String,
    pub file_name: String,
    pub offset: u32,
    pub size: u32,
    pub format: u8,
    pub channels: u8,
    pub rate: u32,
    pub frames: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SoundIndex {
    pub version: u32,
    /// The BO2 folder this index was made from.
    pub bo2_dir: String,
    /// BO2 alias name -> sound.
    pub sounds: BTreeMap<String, SoundRef>,
    pub missing: Vec<String>,
    #[serde(default)]
    pub banks_scanned: usize,
}

pub const INDEX_VERSION: u32 = 1;

/// Every .sabl/.sabs under `dir`, "all" banks first so shared sounds resolve to them.
pub fn find_banks(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
                if ext.eq_ignore_ascii_case("sabl") || ext.eq_ignore_ascii_case("sabs") {
                    out.push(p);
                }
            }
        }
    }
    out.sort_by_key(|p| {
        let n = p.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        (!n.contains(".all."), !n.ends_with(".sabl"), n)
    });
    out
}

/// Where BO2 keeps its sound banks: `sound/` (OpenAssetTools' T6 search paths), and `zone/` to be safe.
pub const BANK_DIRS: &[&str] = &["sound", "zone"];

/// Finds the wanted file names (alias -> FileSource) in the banks under `bo2_dir/sound` and `bo2_dir/zone`.
pub fn locate(bo2_dir: &Path, wanted: &HashMap<String, String>, log: &mut dyn FnMut(String)) -> SoundIndex {
    let mut by_id: HashMap<u32, Vec<&String>> = HashMap::new();
    for (alias, file_source) in wanted {
        by_id.entry(snd_hash_name(file_source_to_asset_name(file_source))).or_default().push(alias);
    }
    let mut index = SoundIndex { version: INDEX_VERSION, bo2_dir: bo2_dir.to_string_lossy().into_owned(), ..Default::default() };
    let mut found: HashSet<u32> = HashSet::new();
    let banks: Vec<PathBuf> = BANK_DIRS.iter().flat_map(|d| find_banks(&bo2_dir.join(d))).collect();
    index.banks_scanned = banks.len();
    for bank in banks {
        if found.len() == by_id.len() {
            break;
        }
        let entries = match read_bank_entries(&bank) {
            Ok(e) => e,
            Err(e) => {
                log(format!("skip bank {}: {e}", bank.display()));
                continue;
            }
        };
        let rel = bank.strip_prefix(bo2_dir).unwrap_or(&bank).to_string_lossy().replace('\\', "/");
        for e in entries {
            if found.contains(&e.id) {
                continue;
            }
            let Some(aliases) = by_id.get(&e.id) else { continue };
            if e.format != FORMAT_FLAC && e.format != FORMAT_PCMS16 {
                log(format!("{rel}: id {:#x} has format {} (not FLAC or PCM16), skipped", e.id, e.format));
                continue;
            }
            found.insert(e.id);
            for alias in aliases {
                index.sounds.insert(
                    (*alias).clone(),
                    SoundRef {
                        bank: rel.clone(),
                        file_name: file_source_to_asset_name(&wanted[*alias]).to_string(),
                        offset: e.offset,
                        size: e.size,
                        format: e.format,
                        channels: e.channel_count,
                        rate: FRAME_RATES.get(e.frame_rate_index as usize).copied().unwrap_or(48000),
                        frames: e.frame_count,
                    },
                );
            }
        }
    }
    index
}

/// Reads one sound's bytes from the player's bank, where it is.
pub fn read_sound(bo2_dir: &Path, s: &SoundRef) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(bo2_dir.join(&s.bank))?;
    f.seek(SeekFrom::Start(s.offset as u64))?;
    let mut buf = vec![0u8; s.size as usize];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// Parses an OpenAssetTools `*.aliases.csv` into (Name, FileSource) pairs.
pub fn parse_alias_csv(text: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut lines = text.lines();
    let Some(header) = lines.next() else { return rows };
    let cols = split_csv_line(header);
    let (Some(ni), Some(fi)) = (
        cols.iter().position(|c| c.eq_ignore_ascii_case("Name")),
        cols.iter().position(|c| c.eq_ignore_ascii_case("FileSource")),
    ) else {
        return rows;
    };
    for line in lines {
        let c = split_csv_line(line);
        if let (Some(n), Some(f)) = (c.get(ni), c.get(fi)) {
            if !n.is_empty() && !f.is_empty() {
                rows.push((n.clone(), f.clone()));
            }
        }
    }
    rows
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_game() {
        // Reference: OpenAssetTools Common::SND_HashName (CommonT6.h), itsmeft24's SND_HashName.
        fn reference(s: &str) -> u32 {
            let mut h: u32 = 5381;
            for b in s.bytes() {
                let c = b.to_ascii_lowercase() as u32;
                h = (h << 16).wrapping_add(h << 6).wrapping_add(c).wrapping_sub(h);
                if h == 0 {
                    h = 1;
                }
            }
            h
        }
        for s in ["mus_zombie_round_start", "zmb/MUS/Round_Start", "a", "evt_perk_deny"] {
            assert_eq!(snd_hash_name(s), reference(s), "{s}");
        }
        assert_eq!(snd_hash_name(""), 0);
        assert_eq!(snd_hash_name("ABC"), snd_hash_name("abc"));
    }

    #[test]
    fn strips_extension() {
        assert_eq!(file_source_to_asset_name("music/zm/round_start.flac"), "music/zm/round_start");
        assert_eq!(file_source_to_asset_name("x/y"), "x/y");
    }

    #[test]
    fn csv() {
        let t = "Name,FileSource,Secondary\nmus_a,\"dir/a,b\",\nmus_b,dir/b,x\n,dir/c,\n";
        let r = parse_alias_csv(t);
        assert_eq!(r, vec![("mus_a".into(), "dir/a,b".into()), ("mus_b".into(), "dir/b".into())]);
    }

    #[test]
    fn synthetic_bank_round_trip() {
        // Build a bank the way the game lays one out and find a sound in it.
        let dir = std::env::temp_dir().join(format!("bo2snd-test-{}", std::process::id()));
        let zone = dir.join("sound");
        std::fs::create_dir_all(&zone).unwrap();
        let audio = b"fLaC-not-really-audio".to_vec();
        let data_off = HEADER_SIZE as u32;
        let entry_off = data_off as u64 + audio.len() as u64;
        let total = entry_off + ENTRY_SIZE as u64 + 16;
        let mut f = vec![0u8; HEADER_SIZE];
        f[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        f[4..8].copy_from_slice(&VERSION.to_le_bytes());
        f[8..12].copy_from_slice(&(ENTRY_SIZE as u32).to_le_bytes());
        f[20..24].copy_from_slice(&1u32.to_le_bytes());
        f[32..40].copy_from_slice(&total.to_le_bytes());
        f[40..48].copy_from_slice(&entry_off.to_le_bytes());
        f.extend_from_slice(&audio);
        let id = snd_hash_name("zmb/mus/round_start");
        f.extend_from_slice(&id.to_le_bytes());
        f.extend_from_slice(&(audio.len() as u32).to_le_bytes());
        f.extend_from_slice(&data_off.to_le_bytes());
        f.extend_from_slice(&1000u32.to_le_bytes());
        f.extend_from_slice(&[6, 2, 0, FORMAT_FLAC]);
        f.extend_from_slice(&[0u8; 16]);
        std::fs::write(zone.join("zm_test.all.sabl"), &f).unwrap();

        let wanted = HashMap::from([("mus_zombie_round_start".to_string(), "zmb/mus/round_start.flac".to_string())]);
        let idx = locate(&dir, &wanted, &mut |m| eprintln!("{m}"));
        let s = &idx.sounds["mus_zombie_round_start"];
        assert_eq!(s.bank, "sound/zm_test.all.sabl");
        assert_eq!(idx.banks_scanned, 1);
        assert_eq!((s.rate, s.channels, s.format), (48000, 2, FORMAT_FLAC));
        assert_eq!(read_sound(&dir, s).unwrap(), audio);
        std::fs::remove_dir_all(&dir).ok();
    }
}
