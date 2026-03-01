use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs;
use std::mem;
use std::path::PathBuf;
use std::ptr;
use windows::{Win32::Devices::Display::*, Win32::Foundation::*};

const DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO: u32 = 9;
const DISPLAYCONFIG_DEVICE_INFO_SET_ADVANCED_COLOR_STATE: u32 = 10;

#[repr(C)]
#[derive(Clone, Copy)]
struct DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    value: u32,
    color_encoding: u32,
    bits_per_color_channel: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DISPLAYCONFIG_SET_ADVANCED_COLOR_STATE {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    value: u32,
}

/// Get all active display paths
fn get_display_paths() -> Result<Vec<DISPLAYCONFIG_PATH_INFO>, Box<dyn std::error::Error>> {
    unsafe {
        let mut path_count: u32 = 0;
        let mut mode_count: u32 = 0;

        let result =
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count);
        if result != ERROR_SUCCESS {
            return Err(format!("GetDisplayConfigBufferSizes failed: {:?}", result).into());
        }

        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![mem::zeroed(); path_count as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![mem::zeroed(); mode_count as usize];

        let result = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            Some(ptr::null_mut()),
        );
        if result != ERROR_SUCCESS {
            return Err(format!("QueryDisplayConfig failed: {:?}", result).into());
        }

        paths.truncate(path_count as usize);
        Ok(paths)
    }
}

/// Check if HDR is enabled on any display
fn get_hdr_status() -> Result<bool, Box<dyn std::error::Error>> {
    let paths = get_display_paths()?;

    unsafe {
        for path in &paths {
            let mut color_info: DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO = mem::zeroed();
            color_info.header.r#type = DISPLAYCONFIG_DEVICE_INFO_TYPE(
                DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO as i32,
            );
            color_info.header.size = mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
            color_info.header.adapterId = path.targetInfo.adapterId;
            color_info.header.id = path.targetInfo.id;

            let result = DisplayConfigGetDeviceInfo(&mut color_info.header);
            if result == 0 {
                // Bit 0: advancedColorSupported
                // Bit 1: advancedColorEnabled
                let advanced_color_enabled = (color_info.value & 0x2) != 0;
                if advanced_color_enabled {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

/// Set HDR mode on all capable displays
fn set_hdr(enable: bool) -> Result<(), Box<dyn std::error::Error>> {
    let paths = get_display_paths()?;
    let mut any_success = false;

    unsafe {
        for path in &paths {
            let mut color_info: DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO = mem::zeroed();
            color_info.header.r#type = DISPLAYCONFIG_DEVICE_INFO_TYPE(
                DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO as i32,
            );
            color_info.header.size = mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
            color_info.header.adapterId = path.targetInfo.adapterId;
            color_info.header.id = path.targetInfo.id;

            let result = DisplayConfigGetDeviceInfo(&mut color_info.header);
            if result != 0 {
                continue;
            }

            // Check if advanced color is supported (bit 0)
            let supported = (color_info.value & 0x1) != 0;
            if !supported {
                continue;
            }

            // Set the HDR state
            let mut set_state: DISPLAYCONFIG_SET_ADVANCED_COLOR_STATE = mem::zeroed();
            set_state.header.r#type = DISPLAYCONFIG_DEVICE_INFO_TYPE(
                DISPLAYCONFIG_DEVICE_INFO_SET_ADVANCED_COLOR_STATE as i32,
            );
            set_state.header.size = mem::size_of::<DISPLAYCONFIG_SET_ADVANCED_COLOR_STATE>() as u32;
            set_state.header.adapterId = path.targetInfo.adapterId;
            set_state.header.id = path.targetInfo.id;
            set_state.value = if enable { 1 } else { 0 };

            let result = DisplayConfigSetDeviceInfo(&set_state.header);
            if result == 0 {
                any_success = true;
            }
        }
    }

    if any_success {
        Ok(())
    } else {
        Err("No HDR-capable displays found or failed to set HDR state".into())
    }
}

#[derive(Parser, Debug)]
#[command(name = "hdr-toggle")]
#[command(about = "Toggle HDR mode on Windows displays", long_about = None)]
struct Args {
    /// Toggle HDR on/off
    #[arg(short, long)]
    toggle: bool,

    /// Turn HDR on
    #[arg(long, conflicts_with = "off")]
    on: bool,

    /// Turn HDR off
    #[arg(long, conflicts_with = "on")]
    off: bool,

    /// Restore to preferred HDR mode
    #[arg(short, long)]
    restore: bool,

    /// Set preferred HDR mode (saves for --restore)
    #[arg(short, long, value_enum)]
    set_preferred: Option<HdrMode>,

    /// Get current HDR status
    #[arg(short, long)]
    get_status: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, Deserialize, PartialEq)]
pub enum HdrMode {
    On,
    Off,
}

impl std::fmt::Display for HdrMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HdrMode::On => write!(f, "on"),
            HdrMode::Off => write!(f, "off"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    preferred_mode: HdrMode,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            preferred_mode: HdrMode::On,
        }
    }
}

fn get_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hdr-toggle");
    fs::create_dir_all(&config_dir).ok();
    config_dir.join("config.json")
}

fn load_config() -> Config {
    let path = get_config_path();
    if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}

fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_config_path();
    let content = serde_json::to_string_pretty(config)?;
    fs::write(path, content)?;
    Ok(())
}

fn set_hdr_mode(mode: HdrMode) -> Result<(), Box<dyn std::error::Error>> {
    let enable = matches!(mode, HdrMode::On);
    println!("Setting HDR mode to: {}", mode);
    set_hdr(enable)?;
    println!("HDR mode set to: {}", mode);
    Ok(())
}

fn toggle_hdr() -> Result<(), Box<dyn std::error::Error>> {
    let current = get_hdr_status().unwrap_or(false);
    let new_mode = if current { HdrMode::Off } else { HdrMode::On };
    println!("Current HDR status: {}", if current { "on" } else { "off" });
    println!("Toggling to: {}", new_mode);
    set_hdr_mode(new_mode)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Handle set-preferred first as it just saves config
    if let Some(mode) = args.set_preferred {
        let mut config = load_config();
        config.preferred_mode = mode;
        save_config(&config)?;
        println!("Preferred HDR mode set to: {}", mode);
        return Ok(());
    }

    // Handle get-status
    if args.get_status {
        let config = load_config();
        match get_hdr_status() {
            Ok(enabled) => {
                println!("HDR is currently: {}", if enabled { "on" } else { "off" });
            }
            Err(e) => {
                eprintln!("Could not query HDR status: {}", e);
            }
        }
        println!("Preferred mode: {}", config.preferred_mode);
        return Ok(());
    }

    // Handle explicit on/off
    if args.on {
        return set_hdr_mode(HdrMode::On);
    }

    if args.off {
        return set_hdr_mode(HdrMode::Off);
    }

    // Handle toggle
    if args.toggle {
        return toggle_hdr();
    }

    // Handle restore
    if args.restore {
        let config = load_config();
        println!("Restoring to preferred mode: {}", config.preferred_mode);
        return set_hdr_mode(config.preferred_mode);
    }

    // If no action specified, show help
    println!("No action specified. Use --help for usage information.");
    println!();
    println!("Quick usage:");
    println!("  hdr-toggle --on             Turn HDR on");
    println!("  hdr-toggle --off            Turn HDR off");
    println!("  hdr-toggle --toggle         Toggle HDR on/off");
    println!("  hdr-toggle --restore        Restore to preferred mode");
    println!("  hdr-toggle --set-preferred on|off  Set preferred mode");
    println!("  hdr-toggle --get-status     Show current HDR status");

    Ok(())
}
