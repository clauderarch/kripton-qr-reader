use std::io::{self, Write};
use std::path::PathBuf;
use anyhow::{Result, Context};
use image::{ImageBuffer, Luma, DynamicImage}; 
use zeroize::Zeroizing;
use serde::{Serialize, Deserialize};
use walkdir::WalkDir;
use dirs;
use arboard::Clipboard;

type AppResult<T> = Result<T>;
const APP_NAME: &str = "kripton-qr-reader";
const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppSettings {
    #[serde(default)] 
    scan_directory: Option<PathBuf>,
    #[serde(default)]
    auto_copy_to_clipboard: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            scan_directory: None,
            auto_copy_to_clipboard: false,
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

fn enhance_contrast(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    let (width, height) = img.dimensions();
    let mut enhanced = ImageBuffer::new(width, height);
    
    let mut histogram = [0u32; 256];
    for pixel in img.pixels() {
        histogram[pixel[0] as usize] += 1;
    }
    
    let total_pixels = (width * height) as f32;
    let mut cdf = [0.0f32; 256];
    let mut sum = 0.0;
    
    for i in 0..256 {
        sum += histogram[i] as f32 / total_pixels;
        cdf[i] = sum;
    }
    
    for (x, y, pixel) in enhanced.enumerate_pixels_mut() {
        let old_val = img.get_pixel(x, y)[0] as usize;
        let new_val = (cdf[old_val] * 255.0) as u8;
        *pixel = Luma([new_val]);
    }
    
    enhanced
}

fn adaptive_threshold(img: &ImageBuffer<Luma<u8>, Vec<u8>>, block_size: u32) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    let (width, height) = img.dimensions();
    let mut result = ImageBuffer::new(width, height);
    let half_block = block_size / 2;
    
    for y in 0..height {
        for x in 0..width {
            let x_start = x.saturating_sub(half_block);
            let x_end = (x + half_block).min(width - 1);
            let y_start = y.saturating_sub(half_block);
            let y_end = (y + half_block).min(height - 1);
            
            let mut sum = 0u32;
            let mut count = 0u32;
            
            for yy in y_start..=y_end {
                for xx in x_start..=x_end {
                    sum += img.get_pixel(xx, yy)[0] as u32;
                    count += 1;
                }
            }
            
            let mean = sum / count;
            let pixel_val = img.get_pixel(x, y)[0] as u32;
            
            let new_val = if pixel_val < mean.saturating_sub(5) { 0 } else { 255 };
            result.put_pixel(x, y, Luma([new_val]));
        }
    }
    
    result
}

fn try_different_scales(img: &DynamicImage) -> Vec<ImageBuffer<Luma<u8>, Vec<u8>>> {
    let mut processed_images = Vec::new();
    
    let img_gray = img.to_luma8();
    processed_images.push(img_gray.clone());
    
    let enhanced = enhance_contrast(&img_gray);
    processed_images.push(enhanced.clone());
    
    let thresholded = adaptive_threshold(&img_gray, 15);
    processed_images.push(thresholded);
    
    let scaled_up = img.resize_exact(
        (img.width() as f32 * 1.5) as u32,
        (img.height() as f32 * 1.5) as u32,
        image::imageops::FilterType::Lanczos3
    ).to_luma8();
    processed_images.push(scaled_up.clone());
    processed_images.push(enhance_contrast(&scaled_up));
    
    if img.width() > 400 && img.height() > 400 {
        let scaled_down = img.resize_exact(
            (img.width() as f32 * 0.8) as u32,
            (img.height() as f32 * 0.8) as u32,
            image::imageops::FilterType::Lanczos3
        ).to_luma8();
        processed_images.push(scaled_down);
    }
    
    processed_images
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

    println!("Processing image with multiple techniques...");

    let processed_images = try_different_scales(&img);

    let mut all_results = Vec::new();

    for (_technique_idx, processed_img) in processed_images.iter().enumerate() {
        let mut prepared_img = rqrr::PreparedImage::prepare(processed_img.clone());
        let grids = prepared_img.detect_grids();

        for grid in grids {
            if let Ok((_metadata, content)) = grid.decode() {
                let content_str = content;

                if !all_results.iter().any(|(_, c)| c == &content_str) {
                    all_results.push((_technique_idx, content_str));
                }
            }
        }
    }

    if all_results.is_empty() {
        println!("No QR code could be decoded from the selected image.");
        println!("Tried {} different processing techniques.", processed_images.len());
        return Ok(());
    }

    println!("\n{} unique QR code(s) successfully decoded!", all_results.len());

    if settings.auto_copy_to_clipboard && all_results.len() == 1 {
    if let Some((_, content)) = all_results.first() {
        let copy_result = (|| -> Result<()> {
            let mut clipboard = Clipboard::new().context("Failed to initialize clipboard")?;
            clipboard.set_text(content.clone())
                .context("Failed to copy content to clipboard")?;
            #[cfg(target_os = "linux")]
            {
                use std::thread;
                use std::time::Duration;
                thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        })();

        if copy_result.is_ok() {
            println!("Content of the QR code has been automatically copied to the clipboard.");
        } else if let Err(e) = copy_result {
            eprintln!("Warning: Could not copy content to clipboard: {:?}", e);
        }
    }
}

    for (i, (_technique, content)) in all_results.iter().enumerate() {
        let zeroized_content = Zeroizing::new(content.clone());

        println!("--- QR Code {} ---", i + 1);
        println!("Content: {}", zeroized_content.as_str());
    }

    Ok(())
}

fn settings_menu(settings: &mut AppSettings) -> AppResult<()> {
    let mut in_settings_menu = true;
    while in_settings_menu {
        println!("\n--- Settings Menu ---");

        match &settings.scan_directory {
            Some(p) => println!("1. Set Scan Directory (Current: {})", p.display()),
            None => println!("1. Set Scan Directory (Current: NOT SET)"),
        }

        let auto_copy_status = if settings.auto_copy_to_clipboard { "Enabled" } else { "Disabled" };
        println!("2. Toggle Auto-copy to Clipboard (Current: {})", auto_copy_status);
        
        println!("3. Back to Main Menu");
        print!("Make your selection (1-3): ");
        io::stdout().flush()?;

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;

        match choice.trim() {
            "1" => {
                print!("Enter New Scan Directory Path (or leave empty to cancel): ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let new_path_str = input.trim();

                if !new_path_str.is_empty() {
                    let path_buf = PathBuf::from(new_path_str);
                    if path_buf.is_dir() {
                        settings.scan_directory = Some(path_buf);
                        println!("Scan directory updated successfully. Saving...");
                        save_settings(settings)?;
                    } else {
                        println!("Error: The entered path is not a valid directory.");
                    }
                } else {
                    println!("No path entered, operation cancelled.");
                }
            },
            "2" => {
                settings.auto_copy_to_clipboard = !settings.auto_copy_to_clipboard;
                let new_status = if settings.auto_copy_to_clipboard { "Enabled" } else { "Disabled" };
                println!("Auto-copy to clipboard is now {}. Saving...", new_status);
                save_settings(settings)?;
            },
            "3" => {
                in_settings_menu = false;
            },
            _ => {
                println!("Invalid selection. Please enter 1, 2, or 3.");
            }
        }
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
        println!("\n--- Kripton QR Code Reader ---");
        println!("1. Read QR Code from Images in Scan Directory");
        println!("2. Settings");
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
                println!("Exiting application...");
                running = false;
            },
            _ => {
                println!("Invalid selection. Please enter 1, 2, or 3.");
            }
        }
    }

    Ok(())
}
