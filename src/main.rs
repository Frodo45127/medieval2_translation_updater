use anyhow::Result;
use clap::builder::PossibleValuesParser;
use clap::Parser;
use deepl::{DeepLApi, Lang};
use std::collections::HashMap;
use std::fs::{self, DirBuilder, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {

    /// Target language code (e.g., ES, DE, FR). If provided and there's a deepl api key configured in the env var "DEEPL_API_KEY", all english lines will be automatically translated.
    #[arg(short = 'l', long, value_parser = PossibleValuesParser::new(["BG", "CS", "DA", "DE", "EL", "EN", "EN-GB", "EN-US", "ES", "ET", "FI", "FR", "HU", "IT", "JA", "LT", "LV", "NL", "PL", "PT", "PT-BR", "PT-PT", "RO", "RU", "SK", "SL", "SV", "ZH"]))]
    lang: Option<String>,

    /// Paths for the translation update process: [OLD_ENG] [NEW_ENG] [OLD_TRAD] [OUTPUT].
    /// If not provided, defaults to paths relative to the executable.
    /// The paths mean:
    /// - OLD_ENG: Path to the english files (the folder containing the txt files) of the version of the mod the original translation is for.
    /// - NEW_ENG: Path to the english files from the updated version of the mod.
    /// - OLD_TRAD: Path to the old translated files, the ones you want to update.
    /// - OUTPUT: Path where the updated translated files will be saved.
    #[arg(num_args = 4)]
    paths: Option<Vec<PathBuf>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting translation update process...");

    let args = Args::parse();

    // Determine paths
    let (path_old_eng, path_new_eng, path_old_trad, path_new_trad) = match args.paths {
        Some(paths) => {
            println!("Using provided paths: [OLD_ENG] [NEW_ENG] [OLD_TRAD] [OUTPUT]");
            (paths[0].clone(), paths[1].clone(), paths[2].clone(), paths[3].clone())
        }
        None => {
            println!("No paths provided. Using default relative paths.");
            default_paths()?
        }
    };

    println!("Folders:");
    println!("eng_old:        {:?}", path_old_eng);
    println!("eng_new:        {:?}", path_new_eng);
    println!("translated_old: {:?}", path_old_trad);
    println!("output:         {:?}", path_new_trad);
    println!("Creating the folders, if they don't exist");

    DirBuilder::new().recursive(true).create(&path_old_eng)?;
    DirBuilder::new().recursive(true).create(&path_new_eng)?;
    DirBuilder::new().recursive(true).create(&path_old_trad)?;
    DirBuilder::new().recursive(true).create(&path_new_trad)?;

    // Setup DeepL translation stuff.
    let translator = if let Some(lang_code) = args.lang {
        let api_key = std::env::var("DEEPL_API_KEY").unwrap_or_default();
        let lang = parse_lang_code(&lang_code);
        if lang.is_some() && !api_key.is_empty() {
            let lang = lang.unwrap();
            println!("DeepL translations enabled. Target Lang: {:?}", lang);
            Some((DeepLApi::with(&api_key).new(), lang))
        } else if lang.is_none() {
            println!("Invalid language code: {}", lang_code);
            None
        } else {
            println!("Missing DeepL api key. DeepL translations disabled.");
            None
        }
    } else {
        println!("No -l arg passed. DeepL translations disabled.");
        None
    };

    println!("Processing files...");
    process_files(&path_old_eng, &path_new_eng, &path_old_trad, &path_new_trad, &translator).await?;

    println!("Files processed successfully.");
    Ok(())
}

fn default_paths() -> Result<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));

    Ok((exe_dir.join("eng_old"), exe_dir.join("eng_new"), exe_dir.join("translated_old"), exe_dir.join("output")))
}

fn parse_lang_code(code: &str) -> Option<Lang> {
    match code.to_uppercase().as_str() {
        "BG" => Some(Lang::BG),
        "CS" => Some(Lang::CS),
        "DA" => Some(Lang::DA),
        "DE" => Some(Lang::DE),
        "EL" => Some(Lang::EL),
        "EN" => Some(Lang::EN_US),
        "EN-GB" => Some(Lang::EN_GB),
        "EN-US" => Some(Lang::EN_US),
        "ES" => Some(Lang::ES),
        "ET" => Some(Lang::ET),
        "FI" => Some(Lang::FI),
        "FR" => Some(Lang::FR),
        "HU" => Some(Lang::HU),
        "IT" => Some(Lang::IT),
        "JA" => Some(Lang::JA),
        "LT" => Some(Lang::LT),
        "LV" => Some(Lang::LV),
        "NL" => Some(Lang::NL),
        "PL" => Some(Lang::PL),
        "PT" => Some(Lang::PT_PT),
        "PT-BR" => Some(Lang::PT_BR),
        "PT-PT" => Some(Lang::PT_PT),
        "RO" => Some(Lang::RO),
        "RU" => Some(Lang::RU),
        "SK" => Some(Lang::SK),
        "SL" => Some(Lang::SL),
        "SV" => Some(Lang::SV),
        "ZH" => Some(Lang::ZH),
        _ => None,
    }
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
        Err(e) => eprintln!("Error reading file {:?}: {}", file_path, e),
    }

    Ok(map)
}

async fn process_files(
    old_eng_base_path: &Path,
    new_eng_base_path: &Path,
    old_trad_base_path: &Path,
    new_trad_base_path: &Path,
    translator: &Option<(DeepLApi, Lang)>,
) -> Result<()> {
    for entry in WalkDir::new(new_eng_base_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let relative_path = entry.path().strip_prefix(new_eng_base_path)?;

            // Construct corresponding paths
            let old_eng_path = old_eng_base_path.join(relative_path);
            let old_trad_path = old_trad_base_path.join(relative_path);
            let output_path = new_trad_base_path.join(relative_path);

            // Load translations specifically for this file. If the paths don't exist, the resulting hasmaps are empty.
            let old_eng = load_file_to_map(&old_eng_path)?;
            let old_trad = load_file_to_map(&old_trad_path)?;

            process_single_file(entry.path(), &output_path, &old_eng, &old_trad, translator).await?;
        }
    }
    Ok(())
}

async fn process_single_file(input_path: &Path, output_path: &Path, old_eng: &HashMap<String, String>, old_trad: &HashMap<String, String>, translator: &Option<(DeepLApi, Lang)>) -> Result<()> {
    let content = match read_utf16_file(input_path) {
        Ok(c) => c,
        Err(e) => {

            // If the file is not UTF-16 encoded, copy it directly to the output path.
            // These files should not be translated.
            if e.to_string().contains("No BOM found") {
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
    let mut translation_cache: HashMap<String, String> = HashMap::new();

    for line in content.lines() {

        // Trim their end, because sometimes the only change is a final space removed,
        // and we can keep using the translations of those lines.
        let trimmed = line.trim_end();

        // In med 2, comments start with '¬', so skip those lines, and also empty lines.
        let new_line_content = if trimmed.starts_with('¬') || trimmed.is_empty() {
            trimmed.to_string()
        } else {
            match parse_line(trimmed) {
                Some((key, val_new)) => {

                    // Skip empty lines.
                    if val_new.is_empty() {
                        trimmed.to_string()
                    } else {

                        // Check if the value is the same as the old english files, in which case we resuse its old translation.
                        // If it's not in the old translation or it is but has changed, keep the new english value.
                        let mut val_to_use = if let Some(val_old) = old_eng.get(&key) {
                            if &val_new == val_old.trim_end() {
                                if let Some(val_trad) = old_trad.get(&key) {

                                    // Value unchanged, but different in the translation => pretranslated.
                                    if &val_new != val_trad.trim_end() {
                                        Some(val_trad.trim_end().to_string())
                                    }

                                    // Value unchanged in all, needs translation.
                                    else {
                                        None
                                    }
                                }

                                // Value missing from the translation, needs translation.
                                else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        // If we don't have a pre-translated line, check if we can translate it with DeepL.
                        if val_to_use.is_none() {
                            if let Some(translation) = translation_cache.get(&val_new) {
                                val_to_use = Some(translation.trim_end().to_string())
                            } else if let Some((client, lang)) = translator {

                                // Split multiline strings, because DeepL has a tendency to eath the \n otherwise.
                                let val_new_split = val_new.replace("\\n", "\n");
                                match client.translate_text(val_new_split.as_str(), lang.clone()).await {
                                    Ok(res) => {
                                        let translated_text = res
                                            .translations
                                            .first()
                                            .map(|s| s.text.trim_end().replace("\n", "\\n"))
                                            .unwrap_or_else(|| val_new.trim_end().to_string());

                                        // Cache the translation so we can re-use it if the line is repeated.
                                        translation_cache.insert(val_new.to_owned(), translated_text.to_owned());
                                        println!("Translated with DeepL. Key: {}, Old val: {}, New val: {}", key, val_new, translated_text);
                                        val_to_use = Some(translated_text);
                                    },
                                    Err(err) => {
                                        println!("Failed to translate with DeepL. Key: {}, Old val: {}, Error: {}", key, val_new, err);

                                        if err.to_string().to_lowercase().contains("too many requests") {
                                            println!("Sleeping 12 seconds to avoid rate limit. Remember to replace the old translation with the output and re-run this to translate the lines that errored out.");
                                            sleep(Duration::from_secs(12));
                                        }
                                    }
                                }
                            }
                        }

                        match val_to_use {
                            Some(translated_val) => {
                                translation_cache.insert(val_new.to_owned(), translated_val.to_owned());
                                format!("{{{}}}{}", key, translated_val)
                            },
                            None => trimmed.to_string()
                        }
                    }
                }
                None => trimmed.to_string(),
            }
        };
        output_lines.push(new_line_content.trim_end().to_owned());
    }

    write_utf16_file(output_path, &output_lines)?;
    println!("Written: {:?}", output_path);

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
    let mut data = Vec::new();

    // Write BOM
    data.write_all(&[0xFF, 0xFE])?;

    for line in lines {

        // Write line content
        for c in line.encode_utf16() {
            data.write_all(&c.to_le_bytes())?;
        }

        // Write CRLF
        for c in "\r\n".encode_utf16() {
            data.write_all(&c.to_le_bytes())?;
        }
    }

    let mut file = File::create(path)?;
    file.write_all(&data)?;
    file.flush()?;

    Ok(())
}
