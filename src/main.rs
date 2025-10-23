use std::io::{self, Write};
use std::path::PathBuf;
use anyhow::{Result, Context};
use image::{ImageBuffer, Luma, DynamicImage}; 
use zeroize::Zeroizing;
use serde::{Serialize, Deserialize};
use walkdir::WalkDir;
use dirs;
use arboard::Clipboard;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

type AppResult<T> = Result<T>;
const APP_NAME: &str = "kripton-qr-reader";
const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppSettings {
    #[serde(default)] 
    scan_directory: Option<PathBuf>,
    #[serde(default)]
    auto_copy_to_clipboard: bool,
    #[serde(default)]
    output_directory: Option<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            scan_directory: None,
            auto_copy_to_clipboard: false,
            output_directory: None,
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

fn compute_integral(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Vec<Vec<u64>> {
    let (width, height) = img.dimensions();
    let w = width as usize;
    let h = height as usize;
    let mut integral = vec![vec![0u64; w + 1]; h + 1];

    for y in 1..=h {
        for x in 1..=w {
            let val = img.get_pixel((x - 1) as u32, (y - 1) as u32)[0] as u64;
            integral[y][x] = val + integral[y - 1][x] + integral[y][x - 1] - integral[y - 1][x - 1];
        }
    }

    integral
}

fn adaptive_threshold(img: &ImageBuffer<Luma<u8>, Vec<u8>>, block_size: u32) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 {
        return ImageBuffer::new(width, height);
    }

    let mut result = ImageBuffer::new(width, height);
    let half_block = block_size / 2;
    let integral = compute_integral(img);

    for y in 0..height as usize {
        for x in 0..width as usize {
            let x_start = x.saturating_sub(half_block as usize);
            let x_end = (x + half_block as usize).min(width as usize - 1);
            let y_start = y.saturating_sub(half_block as usize);
            let y_end = (y + half_block as usize).min(height as usize - 1);

            let count = ((x_end - x_start + 1) * (y_end - y_start + 1)) as u64;
            if count == 0 {
                result.put_pixel(x as u32, y as u32, Luma([128]));
                continue;
            }

            let sum = integral[y_end + 1][x_end + 1]
                .saturating_sub(integral[y_end + 1][x_start])
                .saturating_sub(integral[y_start][x_end + 1])
                .saturating_add(integral[y_start][x_start]);

            let mean = (sum / count) as u32;
            let pixel_val = img.get_pixel(x as u32, y as u32)[0] as u32;

            let new_val = if pixel_val < mean.saturating_sub(5) { 0 } else { 255 };
            result.put_pixel(x as u32, y as u32, Luma([new_val as u8]));
        }
    }

    result
}

fn try_different_scales(img: &DynamicImage) -> Vec<ImageBuffer<Luma<u8>, Vec<u8>>> {
    let mut processed_images = Vec::with_capacity(6);
    
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

fn process_image(path: &PathBuf, _settings: &AppSettings) -> AppResult<Vec<(String, Zeroizing<String>)>> {
    let img = image::open(path)
        .with_context(|| format!("Could not open image file: {}", path.display()))?;

    let processed_images = try_different_scales(&img);
    let mut all_results = Vec::new();

    for (_technique_idx, processed_img) in processed_images.iter().enumerate() {
        let mut prepared_img = rqrr::PreparedImage::prepare(processed_img.clone());
        let grids = prepared_img.detect_grids();

        for grid in grids {
            if let Ok((_metadata, content)) = grid.decode() {
                let content_str = Zeroizing::new(content);
                if !all_results.iter().any(|(_, c)| c == &content_str) {
                    all_results.push((path.display().to_string(), content_str));
                }
            }
        }
    }

    Ok(all_results)
}

fn generate_qr_code(settings: &AppSettings) -> AppResult<()> {
    use qrcode::QrCode;
    use qrcode::render::unicode;
    
    println!("\n--- Generate QR Code ---");
    print!("Enter text to convert to QR code (or leave empty to cancel): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let text = input.trim();

    if text.is_empty() {
        println!("No text entered, operation cancelled.");
        return Ok(());
    }

    let code = QrCode::new(text.as_bytes())
        .context("Could not create QR code. Text may be too long.")?;

    let unicode_image = code.render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build();
    
    println!("\nQR Code (Terminal View):");
    println!("{}", unicode_image);

    print!("\nSave QR code as a PNG file? (Y/N): ");
    io::stdout().flush()?;
    let mut save_choice = String::new();
    io::stdin().read_line(&mut save_choice)?;

    if save_choice.trim().to_lowercase() == "y" {
        let default_dir = settings.output_directory.as_ref()
            .or(settings.scan_directory.as_ref())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        
        print!("Enter file name (default: qr_code.png, directory: {}): ", default_dir);
        io::stdout().flush()?;
        
        let mut filename_input = String::new();
        io::stdin().read_line(&mut filename_input)?;
        let filename = filename_input.trim();
        
        let path = if filename.is_empty() {
            PathBuf::from(&default_dir).join("qr_code.png")
        } else {
            let input_path = PathBuf::from(filename);
            if input_path.is_absolute() {
                input_path
            } else {
                PathBuf::from(&default_dir).join(filename)
            }
        };

        let image = code.render::<image::Luma<u8>>()
            .min_dimensions(200, 200)
            .build();
        
        image.save(&path)
            .context(format!("Could not save QR code file: {}", path.display()))?;
        
        #[cfg(unix)]
        {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                .context(format!("Could not set file permissions: {}", path.display()))?;
        }
        
        println!("QR code saved successfully: {}", path.display());
    }

    Ok(())
}

fn batch_generate_qr_codes(settings: &AppSettings) -> AppResult<()> {
    use qrcode::QrCode;
    
    println!("\n--- Batch QR Code Generation ---");
    print!("Enter path to text file (each line will be a separate QR code): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let file_path_str = input.trim();

    if file_path_str.is_empty() {
        println!("No file path entered, operation cancelled.");
        return Ok(());
    }

    let file_path = PathBuf::from(file_path_str);
    if !file_path.is_file() {
        println!("Error: The provided path is not a valid file.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&file_path)
        .context(format!("Could not read file: {}", file_path.display()))?;

    let lines: Vec<&str> = content.lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        println!("No processable text found in file.");
        return Ok(());
    }

    println!("\n{} lines found. Generating QR codes...", lines.len());

    let default_dir = settings.output_directory.as_ref()
        .or(settings.scan_directory.as_ref())
        .map(|p| p.clone())
        .unwrap_or_else(|| PathBuf::from("."));

    print!("Output directory (default: {}): ", default_dir.display());
    io::stdout().flush()?;
    
    let mut dir_input = String::new();
    io::stdin().read_line(&mut dir_input)?;
    let output_dir = if dir_input.trim().is_empty() {
        default_dir
    } else {
        PathBuf::from(dir_input.trim())
    };

    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir)
            .context(format!("Could not create output directory: {}", output_dir.display()))?;
    }

    let mut success_count = 0;
    let mut error_count = 0;

    for (i, line) in lines.iter().enumerate() {
        let filename = format!("qr_code_{:03}.png", i + 1);
        let path = output_dir.join(&filename);

        match QrCode::new(line.as_bytes()) {
            Ok(code) => {
                let image = code.render::<image::Luma<u8>>()
                    .min_dimensions(200, 200)
                    .build();
                
                match image.save(&path) {
                    Ok(_) => {
                        println!("✓ {} created: {}", filename, &line[..line.len().min(50)]);
                        success_count += 1;
                        
                        #[cfg(unix)]
                        {
                            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Could not save {}: {:?}", filename, e);
                        error_count += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ Could not generate QR code for line {}: {:?}", i + 1, e);
                error_count += 1;
            }
        }
    }

    println!("\nCompleted! Success: {}, Failed: {}", success_count, error_count);
    println!("QR codes saved to: {}", output_dir.display());

    Ok(())
}

fn save_qr_content(contents: &[(String, Zeroizing<String>)], settings: &AppSettings) -> AppResult<()> {
    print!("Enter file path to save QR contents (default: 'qr_batch_output.txt'): ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let path = if input.trim().is_empty() {
        settings.scan_directory.as_ref()
            .map(|p| p.join("qr_batch_output.txt"))
            .unwrap_or_else(|| PathBuf::from("qr_batch_output.txt"))
    } else {
        PathBuf::from(input.trim())
    };

    let mut output = Zeroizing::new(String::new());
    for (i, (file_path, content)) in contents.iter().enumerate() {
        output.push_str(&format!("--- QR Code {} / {} ---\n", i + 1, file_path));
        output.push_str(&format!("Content: {}\n\n", content.as_str()));
    }

    std::fs::write(&path, output.as_bytes())
        .context(format!("Could not write QR contents to file: {}", path.display()))?;
    
    #[cfg(unix)]
    {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .context(format!("Could not set file permissions: {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        println!("Warning: Could not set file permissions (not supported on this platform).");
    }
    
    println!("QR contents saved to: {}", path.display());
    Ok(())
}

fn batch_process_qr_codes(settings: &AppSettings) -> AppResult<()> {
    println!("\n--- Batch QR Code Processing ---");
    let default_dir = settings.scan_directory.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or("Not set".to_string());
    println!("Current scan directory: {}", default_dir);
    print!("Enter new scan directory (press Enter to use current): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let scan_dir = if input.trim().is_empty() {
        match &settings.scan_directory {
            Some(p) => p.clone(),
            None => {
                println!("Error: Scan directory is not set. Please set a directory in Settings.");
                return Ok(());
            }
        }
    } else {
        let new_dir = PathBuf::from(input.trim());
        if !new_dir.is_dir() {
            println!("Error: The provided path is not a valid directory.");
            return Ok(());
        }
        new_dir
    };

    let supported_extensions = &["png", "jpg", "jpeg", "bmp", "gif", "webp"];
    let mut files = Vec::new();

    for entry in WalkDir::new(&scan_dir)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok()) {
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
        println!("No supported image files found in directory (Supported: {:?}).", supported_extensions);
        return Ok(());
    }

    files.sort();
    println!("\nFound {} images in '{}'. Processing...", files.len(), scan_dir.display());
    let mut all_results = Vec::new();

    for (i, path) in files.iter().enumerate() {
        println!("Processing image {}/{}: {}", i + 1, files.len(), path.display());
        match process_image(path, settings) {
            Ok(results) => {
                if results.is_empty() {
                    println!("No QR code found in {}.", path.display());
                } else {
                    all_results.extend(results);
                }
            }
            Err(e) => println!("Error processing {}: {:?}", path.display(), e),
        }
    }

    if all_results.is_empty() {
        println!("\nNo QR codes could be decoded from the images.");
        return Ok(());
    }

    println!("\nSuccessfully decoded {} unique QR code(s)!", all_results.len());
    if settings.auto_copy_to_clipboard && all_results.len() == 1 {
        if let Some((_, content)) = all_results.first() {
            let copy_result = (|| -> Result<()> {
                let mut clipboard = Clipboard::new().context("Could not initialize clipboard")?;
                clipboard.set_text(content.as_str().to_string())
                    .context("Could not copy content to clipboard")?;
                #[cfg(target_os = "linux")]
                {
                    use std::thread;
                    use std::time::Duration;
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(())
            })();

            if copy_result.is_ok() {
                println!("Content of the single QR code was automatically copied to the clipboard.");
            } else if let Err(e) = copy_result {
                eprintln!("Warning: Could not copy content to clipboard: {:?}", e);
            }
        }
    }

    for (i, (file_path, content)) in all_results.iter().enumerate() {
        println!("--- QR Code {} / {} ---", i + 1, file_path);
        println!("Content: {}", content.as_str());
    }

    print!("\nDo you want to save the QR code contents to a file? (Y/N): ");
    io::stdout().flush()?;
    let mut save_choice = String::new();
    io::stdin().read_line(&mut save_choice)?;
    if save_choice.trim().to_lowercase() == "y" {
        if let Err(e) = save_qr_content(&all_results, settings) {
            eprintln!("Error saving QR contents: {:?}", e);
        }
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
        println!("2. Toggle Auto-Copy to Clipboard (Current: {})", auto_copy_status);
        
        match &settings.output_directory {
            Some(p) => println!("3. Set Output Directory (Current: {})", p.display()),
            None => println!("3. Set Output Directory (Current: Scan directory will be used)"),
        }
        
        println!("4. Return to Main Menu");
        print!("Enter your choice (1-4): ");
        io::stdout().flush()?;

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;

        match choice.trim() {
            "1" => {
                print!("Enter new Scan Directory path (leave empty to cancel): ");
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
                        println!("Error: The provided path is not a valid directory.");
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
                print!("Enter new Output Directory path (leave empty for default): ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let new_path_str = input.trim();

                if !new_path_str.is_empty() {
                    let path_buf = PathBuf::from(new_path_str);
                    if path_buf.is_dir() {
                        settings.output_directory = Some(path_buf);
                        println!("Output directory updated successfully. Saving...");
                        save_settings(settings)?;
                    } else {
                        println!("Error: The provided path is not a valid directory.");
                    }
                } else {
                    settings.output_directory = None;
                    println!("Output directory reset to default. Saving...");
                    save_settings(settings)?;
                }
            },
            "4" => {
                in_settings_menu = false;
            },
            _ => {
                println!("Invalid choice. Please enter 1, 2, 3, or 4.");
            }
        }
    }
    Ok(())
}

fn main() -> AppResult<()> {
    let mut settings = match load_settings() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Critical error loading settings: {:?}", e);
            AppSettings::default()
        }
    };
    
    let mut running = true;

    while running {
        println!("\n--- Kripton QR Code Reader/Generator ---");
        println!("1. Read QR Code from Image in Scan Directory");
        println!("2. Read QR Code from Specific File");
        println!("3. Batch Process QR Codes");
        println!("4. Generate QR Code from Text");
        println!("5. Batch Generate QR Codes (from Text File)");
        println!("6. Settings");
        println!("7. Exit");
        print!("Enter your choice (1-7): ");
        io::stdout().flush()?; 

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)
            .context("Failed to read input.")?;
        
        match choice.trim() {
            "1" => {
                if let Err(e) = read_qr_code(&settings) {
                    eprintln!("Error: QR code reading failed: {:?}", e);
                }
            },
            "2" => {
                if let Err(e) = read_qr_from_file(&settings) {
                    eprintln!("Error: QR code reading failed: {:?}", e);
                }
            },
            "3" => {
                if let Err(e) = batch_process_qr_codes(&settings) {
                    eprintln!("Error: Batch QR processing failed: {:?}", e);
                }
            },
            "4" => {
                if let Err(e) = generate_qr_code(&settings) {
                    eprintln!("Error: QR code generation failed: {:?}", e);
                }
            },
            "5" => {
                if let Err(e) = batch_generate_qr_codes(&settings) {
                    eprintln!("Error: Batch QR generation failed: {:?}", e);
                }
            },
            "6" => {
                if let Err(e) = settings_menu(&mut settings) {
                    eprintln!("Error: Failed to change settings: {:?}", e);
                }
            },
            "7" => {
                println!("Exiting application...");
                running = false;
            },
            _ => {
                println!("Invalid choice. Please enter 1, 2, 3, 4, 5, 6, or 7.");
            }
        }
    }

    Ok(())
}

fn read_qr_code(settings: &AppSettings) -> AppResult<()> {
    let scan_dir = match &settings.scan_directory {
        Some(p) => p,
        None => {
            println!("Error: Please set the scan directory from menu 6 first.");
            return Ok(());
        }
    };

    println!("Scan Directory: {}", scan_dir.display());
    let supported_extensions = &["png", "jpg", "jpeg", "bmp", "gif", "webp"];
    let mut files = Vec::new();

    for entry in WalkDir::new(scan_dir)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok()) {
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
        println!("No supported image files found in directory (Supported: {:?}).", supported_extensions);
        return Ok(());
    }

    files.sort();
    println!("\nFound Images (Alphabetical Order):");
    for (i, file) in files.iter().enumerate() {
        let file_name = file.file_name().unwrap_or_default().to_string_lossy();
        println!("{}. {}", i + 1, file_name);
    }

    print!("Enter the number of the image to read (1-{}): ", files.len());
    io::stdout().flush()?;

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;

    let index: usize = match choice.trim().parse::<usize>() {
        Ok(n) if n > 0 && n <= files.len() => n - 1,
        _ => {
            println!("Invalid choice.");
            return Ok(());
        }
    };

    let path = &files[index];
    let results = process_image(path, settings)?;
    if results.is_empty() {
        println!("Could not decode QR code from selected image.");
        println!("{} different processing techniques were tried.", try_different_scales(&image::open(path)?).len());
        return Ok(());
    }

    println!("\nSuccessfully decoded {} unique QR code(s)!", results.len());
    if settings.auto_copy_to_clipboard && results.len() == 1 {
        if let Some((_, content)) = results.first() {
            let copy_result = (|| -> Result<()> {
                let mut clipboard = Clipboard::new().context("Could not initialize clipboard")?;
                clipboard.set_text(content.as_str().to_string())
                    .context("Could not copy content to clipboard")?;
                #[cfg(target_os = "linux")]
                {
                    use std::thread;
                    use std::time::Duration;
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(())
            })();

            if copy_result.is_ok() {
                println!("Content of the single QR code was automatically copied to the clipboard.");
            } else if let Err(e) = copy_result {
                eprintln!("Warning: Could not copy content to clipboard: {:?}", e);
            }
        }
    }

    for (i, (file_path, content)) in results.iter().enumerate() {
        println!("--- QR Code {} / {} ---", i + 1, file_path);
        println!("Content: {}", content.as_str());
    }

    Ok(())
}

fn read_qr_from_file(settings: &AppSettings) -> AppResult<()> {
    print!("Enter the full path to the image file (leave empty to cancel): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let path_str = input.trim();
    if path_str.is_empty() {
        println!("No path entered, operation cancelled.");
        return Ok(());
    }

    let path = PathBuf::from(path_str);
    if !path.is_file() {
        println!("Error: The provided path is not a valid file.");
        return Ok(());
    }

    let supported_extensions = &["png", "jpg", "jpeg", "bmp", "gif", "webp"];
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if !supported_extensions.contains(&ext.to_lowercase().as_str()) {
            println!("Unsupported file extension. Supported: {:?}.", supported_extensions);
            return Ok(());
        }
    } else {
        println!("Could not find file extension.");
        return Ok(());
    }

    let results = process_image(&path, settings)?;
    if results.is_empty() {
        println!("Could not decode QR code from selected image.");
        println!("{} different processing techniques were tried.", try_different_scales(&image::open(&path)?).len());
        return Ok(());
    }

    println!("\nSuccessfully decoded {} unique QR code(s)!", results.len());
    if settings.auto_copy_to_clipboard && results.len() == 1 {
        if let Some((_, content)) = results.first() {
            let copy_result = (|| -> Result<()> {
                let mut clipboard = Clipboard::new().context("Could not initialize clipboard")?;
                clipboard.set_text(content.as_str().to_string())
                    .context("Could not copy content to clipboard")?;
                #[cfg(target_os = "linux")]
                {
                    use std::thread;
                    use std::time::Duration;
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(())
            })();

            if copy_result.is_ok() {
                println!("Content of the single QR code was automatically copied to the clipboard.");
            } else if let Err(e) = copy_result {
                eprintln!("Warning: Could not copy content to clipboard: {:?}", e);
            }
        }
    }

    for (i, (file_path, content)) in results.iter().enumerate() {
        println!("--- QR Code {} / {} ---", i + 1, file_path);
        println!("Content: {}", content.as_str());
    }

    Ok(())
}
