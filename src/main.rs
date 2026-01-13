use anyhow::Result;
use std::collections::HashMap;
use std::fs::{self, DirBuilder, File};
use std::io::{Read, Write};
use std::path::Path;
use walkdir::WalkDir;

fn main() -> Result<()> {
    println!("Iniciando proceso de actualización de traducciones...");

    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));

    // Define paths relative to executable:
    // UPD_TRAD -> translated_old
    // NEW_TRAD -> output
    // OLD_TRAD -> eng_old
    // V3 -> eng_new
    let path_old_trad = exe_dir.join("translated_old");
    let path_new_trad = exe_dir.join("output");
    let path_old_eng = exe_dir.join("eng_old");
    let path_new_eng = exe_dir.join("eng_new");

    println!("Folders:");
    println!("eng_old:        {:?}", path_old_eng);
    println!("eng_new:        {:?}", path_new_eng);
    println!("translated_old: {:?}", path_old_trad);
    println!("output:         {:?}", path_new_trad);

    DirBuilder::new().recursive(true).create(&path_old_eng)?;
    DirBuilder::new().recursive(true).create(&path_new_eng)?;
    DirBuilder::new().recursive(true).create(&path_old_trad)?;
    DirBuilder::new().recursive(true).create(&path_new_trad)?;

    // Process V3 files
    println!("Procesando archivos desde V3...");
    process_v3_files(&path_old_eng, &path_new_eng, &path_old_trad, &path_new_trad)?;

    println!("Proceso finalizado correctamente.");
    Ok(())
}

fn load_file_to_map(file_path: &Path) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();

    if !file_path.exists() {
        return Ok(map);
    }

    match read_utf16_file(file_path) {
        Ok(content) => {
            for line in content.lines() {
                if let Some((key, val)) = parse_line(line) {
                    map.insert(key, val);
                }
            }
        }
        Err(e) => eprintln!("Error leyendo archivo {:?}: {}", file_path, e),
    }

    Ok(map)
}

fn process_v3_files(
    old_eng_base_path: &Path,
    new_eng_base_path: &Path,
    old_trad_base_path: &Path,
    new_trad_base_path: &Path,
) -> Result<()> {
    for entry in WalkDir::new(new_eng_base_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let relative_path = entry.path().strip_prefix(new_eng_base_path)?;

            // Construct corresponding paths
            let old_eng_path = old_eng_base_path.join(relative_path);
            let old_trad_path = old_trad_base_path.join(relative_path);
            let output_path = new_trad_base_path.join(relative_path);

            // Load translations specifically for this file
            let old_eng = load_file_to_map(&old_eng_path)?;
            let old_trad = load_file_to_map(&old_trad_path)?;

            process_single_file(entry.path(), &output_path, &old_eng, &old_trad);
        }
    }
    Ok(())
}

fn process_single_file(
    input_path: &Path,
    output_path: &Path,
    old_eng: &HashMap<String, String>,
    old_trad: &HashMap<String, String>,
) -> Result<()> {
    let content = match read_utf16_file(input_path) {
        Ok(c) => c,
        Err(e) => {
            if e.to_string().contains("No BOM found") {
                // If no BOM, copy file directly to output
                if let Some(parent) = output_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(input_path, output_path)?;
                return Ok(());
            }
            return Err(e);
        }
    };
    let mut output_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_end();
        // Check if commented or empty
        // Prompt: "u con ¬ para marcar líneas comentadas"
        // Prompt logic apply only "por cada línea".
        // Implicitly we preserve comments/structure.

        let new_line_content = if trimmed.starts_with('¬') || trimmed.is_empty() {
            trimmed.to_string()
        } else {
            match parse_line(line) {
                Some((key, val_v3)) => {
                    // Logic:
                    // Si val_v3 != oldTrad[key] OR !oldTrad.contains(key) => Keep val_v3 (so keep line as is)
                    // Si val_v3 == oldTrad[key] =>
                    //    Check updTrad[key]. If exists, select it.
                    let use_translation = if let Some(val_old) = old_eng.get(&key) {
                        if &val_v3 == val_old {
                            // Candidate for translation
                            old_trad.get(&key)
                        } else {
                            // Changed in V3, keep V3
                            None
                        }
                    } else {
                        // Not in old, keep V3
                        None
                    };

                    if let Some(translated_val) = use_translation {
                        // Reconstruct line with translated value
                        // Format: {KEY}Value
                        // Use format! to rebuild
                        format!("{{{}}}{}", key, translated_val)
                    } else {
                        line.to_string()
                    }
                }
                None => trimmed.to_string(), // Could not parse key, keep line
            }
        };
        output_lines.push(new_line_content);
    }

    // Write output to file in UTF-16LE with BOM
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    write_utf16_file(output_path, &output_lines)?;
    println!("Escrito: {:?}", output_path);

    Ok(())
}

// Helper to parse "{KEY}Value"
fn parse_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    if let Some(end_idx) = line.find('}') {
        let key = &line[1..end_idx];
        let val = &line[end_idx + 1..];
        return Some((key.to_string(), val.to_string()));
    }
    None
}

// Helper to read UTF-16LE BOM file to String
fn read_utf16_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;

    if raw.len() < 2 {
        return Ok(String::new());
    }

    // Check BOM (FF FE)
    let start = if raw[0] == 0xFF && raw[1] == 0xFE {
        2
    } else {
        return Err(anyhow::anyhow!("No BOM found, skipping file"));
    };

    let u16s: Vec<u16> = raw[start..]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    String::from_utf16(&u16s).map_err(|e| anyhow::anyhow!("UTF-16 Error: {}", e))
}

// Helper to write String lines to UTF-16LE BOM file
fn write_utf16_file(path: &Path, lines: &[String]) -> Result<()> {
    let mut file = File::create(path)?;

    // Write BOM
    file.write_all(&[0xFF, 0xFE])?;

    for line in lines {
        // Write line content
        for c in line.encode_utf16() {
            file.write_all(&c.to_le_bytes())?;
        }

        // Write CRLF
        for c in "\r\n".encode_utf16() {
            file.write_all(&c.to_le_bytes())?;
        }
    }

    // Remove last CRLF
    //file.set_len(file.metadata()?.len() - 2)?;

    Ok(())
}
