# Kripton QR Code Reader

Kripton QR Code Reader is a Rust-based command-line application designed to read and decode QR codes from image files. It supports various image formats, applies advanced image processing techniques to improve QR code detection, and provides options for batch processing and clipboard integration. The application is secure, user-friendly, and configurable, with settings stored in a JSON file.

## Features

- **QR Code Scanning**: Decode QR codes from individual image files or a directory of images.
- **Batch Processing**: Scan multiple images in a specified directory for QR codes.
- **Image Enhancement**: Uses contrast enhancement and adaptive thresholding to improve QR code detection in low-quality images.
- **Clipboard Support**: Automatically copy QR code content to the clipboard (optional).
- **Configurable Settings**: Save and load settings such as scan directory and auto-copy preferences.
- **Secure Output**: Supports secure file handling with restricted permissions on Unix systems and zeroized memory for sensitive data.
- **Supported Formats**: Works with PNG, JPG, JPEG, BMP, GIF, and WebP image files.

## Installation

### From Source

1. **Prerequisites**:

   - Rust (stable, latest version recommended)
   - On Debian/Ubuntu, install Rust with:

     ```bash
     sudo apt-get install rustc cargo
     ```
   - On Fedora:

     ```bash
     sudo dnf install rust cargo
     ```

2. **Clone and Build**:

   ```bash
   git clone https://github.com/clauderarch/kripton-qr-reader.git
   cd kripton-qr-reader
   cargo build --release
   ```

3. **Run**:

   ```bash
   ./target/release/kripton-qr-reader
   ```

### From AUR (Arch Linux)

Kripton QR Code Reader is available on the Arch User Repository (AUR) in two packages:

- `kripton-qr-reader`: Builds the application from source. Requires Rust and other build dependencies.

  ```bash
  yay -S kripton-qr-reader
  ```

- `kripton-qr-reader-bin`: Installs a precompiled binary, ideal for quick setup without building from source.

  ```bash
  yay -S kripton-qr-reader-bin
  ```

Use an AUR helper like `yay` or `paru` to install these packages.

## Usage

Run the application with:

```bash
kripton-qr-reader
```

### Main Menu Options

1. **Read QR Code from Images in Scan Directory**:

   - Scans images in the configured scan directory.
   - Lists images alphabetically and prompts for selection.
   - Decodes and displays QR code contents.

2. **Read QR Code from a Specific File**:

   - Prompts for the full path to an image file.
   - Decodes and displays QR code contents.

3. **Batch Process QR Codes**:

   - Scans all supported images in the specified directory.
   - Displays decoded QR codes and offers to save results to a file.

4. **Settings**:

   - Configure the scan directory.
   - Toggle auto-copy to clipboard for single QR code results.

5. **Exit**:

   - Closes the application.

### Settings

Settings are stored in `~/.local/share/kripton-qr-reader/settings.json` (or equivalent data directory for your OS). You can configure:

- **Scan Directory**: The default directory for scanning images.
- **Auto-copy to Clipboard**: Automatically copies the content of a single decoded QR code to the clipboard.

### Example

1. Set a scan directory in the Settings menu.
2. Use option 1 to select and scan an image from the directory.
3. Use option 3 to batch process all images in the directory.
4. If auto-copy is enabled and a single QR code is found, its content is copied to the clipboard.
5. Save decoded QR code contents to a file when prompted.

## Dependencies

The application relies on the following Rust crates:

- `image`: For image processing and loading.
- `rqrr`: For QR code detection and decoding.
- `serde` and `serde_json`: For settings serialization.
- `zeroize`: For secure handling of sensitive data.
- `arboard`: For clipboard integration.
- `walkdir`: For directory traversal.
- `dirs`: For accessing user data directories.
- `anyhow`: For error handling.

## Image Processing

The application applies the following techniques to improve QR code detection:

- **Grayscale Conversion**: Converts images to grayscale for processing.
- **Contrast Enhancement**: Uses histogram equalization to improve contrast.
- **Adaptive Thresholding**: Applies block-based thresholding for better QR code visibility.
- **Multi-scale Processing**: Processes images at different scales (original, 1.5x, 0.8x) to handle varying QR code sizes.

## Security Features

- **Zeroized Memory**: Uses `zeroize` to securely clear sensitive data (e.g., QR code contents) from memory.
- **File Permissions**: On Unix systems, output files are set to `600` permissions to restrict access.
- **No External Dependencies**: Avoids external network calls or unsafe operations.

## Contributing

Contributions are welcome!

## License

This project is licensed under the GPL3 License. See the `LICENSE` file for details.

## Acknowledgements

- Built with Rust and inspired by the need for a secure, efficient QR code reader.
- Thanks to the Rust community and crate authors for their excellent libraries.