use std::io::{self, Write};
use std::path::PathBuf;
use anyhow::{Result, Context};
use image::{ImageBuffer, Luma}; 
use zeroize::Zeroizing;
use serde::{Serialize, Deserialize};
use walkdir::WalkDir;
use dirs;

type AppResult<T> = Result<T>;
const APP_NAME: &str = "kripton-qr-reader";
const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppSettings {
    #[serde(default)] 
    scan_directory: Option<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            scan_directory: None,
        }
    }
}

fn get_settings_path() -> AppResult<PathBuf> {
    let mut path = dirs::data_dir()
        .context("User data directory not found.")?;
    
    path.push(APP_NAME);
    if !path.exists() {
        std::fs::create_dir_all(&path)
            .context(format!("Could not create settings directory: {}", path.display()))?;
    }
    
    path.push(SETTINGS_FILENAME);
    Ok(path)
}
fn load_settings() -> AppResult<AppSettings> {
    let settings_path = get_settings_path()?;

    if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .context(format!("Could not read settings file: {}", settings_path.display()))?;
        let settings: AppSettings = serde_json::from_str(&content)
            .context("Settings file format is invalid.")?;
        Ok(settings)
    } else {
        println!("Settings file ({}) not found, using default settings.", settings_path.display());
        Ok(AppSettings::default())
    }
}

fn save_settings(settings: &AppSettings) -> AppResult<()> {
    let settings_path = get_settings_path()?;
    
    let content = serde_json::to_string_pretty(settings)
        .context("Could not convert settings to JSON format.")?;
    
    std::fs::write(&settings_path, content)
        .context(format!("Could not write settings to file: {}", settings_path.display()))?;
    Ok(())
}

fn read_qr_code(settings: &AppSettings) -> AppResult<()> {
    let scan_dir = match &settings.scan_directory {
        Some(p) => p,
        None => {
            println!("Error: Please set the scan directory first from menu 2.");
            return Ok(());
        }
    };

    println!("Scan Directory: {}", scan_dir.display());
    let supported_extensions = &["png", "jpg", "jpeg", "bmp", "gif", "webp"];
    let mut files = Vec::new();

    for entry in WalkDir::new(scan_dir).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if supported_extensions.contains(&ext.to_lowercase().as_str()) {
                    files.push(path.to_path_buf());
                }
            }
        }
    }

    if files.is_empty() {
        println!("No supported image files found in the directory (Supported: {:?}).", supported_extensions);
        return Ok(());
    }
    files.sort();
    println!("\nFound Images (Alphabetical Order):");
    for (i, file) in files.iter().enumerate() {
        let file_name = file.file_name().unwrap_or_default().to_string_lossy();
        println!("{}. {}", i + 1, file_name);
    }
    
    print!("Please enter the number of the image to read (1-{}): ", files.len());
    io::stdout().flush()?;
    
    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    
    let index: usize = match choice.trim().parse::<usize>() {
        Ok(n) if n > 0 && n <= files.len() => n - 1,
        _ => {
            println!("Invalid selection.");
            return Ok(());
        }
    };

    let path = &files[index];
    println!("Selected image: {}", path.display());

    let img = image::open(path)
        .with_context(|| format!("Could not open image file: {}", path.display()))?;

    let img_gray: ImageBuffer<Luma<u8>, Vec<u8>> = img.to_luma8();
    
    let mut prepared_img = rqrr::PreparedImage::prepare(img_gray);
    let grids = prepared_img.detect_grids(); 

    if grids.is_empty() {
        println!("No QR code found in the selected image.");
        return Ok(());
    }

    println!("{} QR codes found.", grids.len());

    for (i, grid) in grids.into_iter().enumerate() {
        match grid.decode() {
            Ok((_metadata, content)) => {
                let zeroized_content: Zeroizing<Vec<u8>> = Zeroizing::from(content.into_bytes());

                let decoded_text = std::str::from_utf8(&zeroized_content[..]) 
                    .context("QR code content is not valid UTF-8.")?;

                println!("--- QR Code {} ---", i + 1);
                println!("Content: {}", decoded_text);
            },
            Err(e) => {
                eprintln!("QR Code {} could not be decoded: {:?}", i + 1, e);
            }
        }
    }

    Ok(())
}

fn settings_menu(settings: &mut AppSettings) -> AppResult<()> {
    println!("\n--- Settings ---");
    
    match &settings.scan_directory {
        Some(p) => println!("Current Scan Directory: {}", p.display()),
        None => println!("Current Scan Directory: NOT SET"),
    }
    
    print!("Enter New Scan Directory Path: ");
    io::stdout().flush()?; 

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let new_path_str = input.trim();

    if !new_path_str.is_empty() {
        let path_buf = PathBuf::from(new_path_str);
        if path_buf.is_dir() {
            settings.scan_directory = Some(path_buf);
            println!("Scan directory set successfully. Saving...");
            save_settings(settings)?;
        } else {
             println!("Error: The entered path is not a valid directory.");
        }
    } else {
        println!("Input was left empty, setting not changed.");
    }

    Ok(())
}

fn main() -> AppResult<()> {
    let mut settings = match load_settings() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Critical error while loading settings: {:?}", e);
            AppSettings::default()
        }
    };
    
    let mut running = true;

    while running {
        println!("\n--- QR Code Reader CLI ---");
        println!("1. Read QR Code from Images in Scan Directory");
        println!("2. Scan Directory Settings");
        println!("3. Exit");
        print!("Make your selection (1-3): ");
        io::stdout().flush()?; 

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)
            .context("Could not read input.")?;
        
        match choice.trim() {
            "1" => {
                if let Err(e) = read_qr_code(&settings) {
                    eprintln!("Error: QR code reading failed: {:?}", e);
                }
            },
            "2" => {
                if let Err(e) = settings_menu(&mut settings) {
                    eprintln!("Error: Could not change settings: {:?}", e);
                }
            },
            "3" => {
                println!("Saving settings and exiting application...");
                save_settings(&settings)?;
                running = false;
            },
            _ => {
                println!("Invalid selection. Please enter 1, 2, or 3.");
            }
        }
    }

    Ok(())
}
