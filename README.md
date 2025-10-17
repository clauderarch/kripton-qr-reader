# Kripton QR Code Reader CLI

Kripton QR Code Reader is a simple Command Line Interface (CLI) application designed to read QR codes embedded within image files. It utilizes image processing techniques, particularly **Adaptive Thresholding**, to improve the readability of challenging QR codes, such as those with embedded logos or low contrast.

## Features

* **Directory Scanning:** Lists all supported image files (PNG, JPEG, etc.) within a configured directory.
* **Robust QR Reading:** Uses the reliable `quircs` engine combined with custom pre-processing (Adaptive Thresholding) to enhance the decoding of poor-quality or stylized QR codes.
* **Persistent Settings:** Saves your scan directory settings permanently in a standard operating system data directory (e.g., `$HOME/.local/share/kripton-qr-reader/settings.json` on Linux).
* **Fully Local:** It works without needing even the slightest internet connection. It keeps your data safe.
* **Auto Copying Feature:** You can copy your QR code content automaticly. You can enable or disable this feature on the settings menu.

## Installation

### Prerequisites

* [Rust programming language](https://www.rust-lang.org/tools/install) (and Cargo) must be installed.

### Arch Linux(AUR)

  ```bash
yay -S kripton-qr-reader
```
or
```bash
yay -S kripton-qr-reader-bin
```

### Other Linux distros

1.  **Clone the Repository:**
    ```bash
    git clone https://github.com/clauderarch/kripton-qr-reader.git
    cd kripton-qr-reader
    ```

2.  **Build and Install:**
    Use `cargo install` to build the application and install it to your Cargo bin directory (usually `$HOME/.cargo/bin`):
    ```bash
    cargo install --path .
    ```

## Usage

Start the application from your terminal using the command:

```bash
kripton-qr-reader
```
or you can use on launcher.

The application will present you with the main menu:

```bash
--- QR Code Reader CLI ---
1. Read QR Code from Images in Scan Directory
2. Settings
3. Exit
Make your selection (1-3):
```
### 1. Settings
Select 1 to configure the directory where your images are located. Enter the full path to your image folder. This setting will be saved permanently unless you change it again.
You can enable auto copying feature here.
### 2. Read QR Code
After setting the scan directory, select 1. The application will list all supported images in that folder.
1. Enter the number corresponding to the image you want to scan.
2. The application will load, process, and display the decoded QR code content.

## License
This project is licensed under the GPL-3.0 License. See the LICENSE file for details.
