use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};

#[derive(Clone, Debug)]
pub struct DeviceInventory {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub default_input: Option<usize>,
    pub default_output: Option<usize>,
}

pub fn discover_devices() -> Result<DeviceInventory> {
    let host = cpal::default_host();

    let mut inputs = Vec::new();
    for device in host.input_devices().context("failed to enumerate input devices")? {
        inputs.push(device.name().unwrap_or_else(|_| "Unnamed input".to_string()));
    }

    let mut outputs = Vec::new();
    for device in host.output_devices().context("failed to enumerate output devices")? {
        outputs.push(device.name().unwrap_or_else(|_| "Unnamed output".to_string()));
    }

    let default_input = host
        .default_input_device()
        .and_then(|device| device.name().ok())
        .and_then(|name| inputs.iter().position(|candidate| candidate == &name));

    let default_output = host
        .default_output_device()
        .and_then(|device| device.name().ok())
        .and_then(|name| outputs.iter().position(|candidate| candidate == &name));

    Ok(DeviceInventory {
        inputs,
        outputs,
        default_input,
        default_output,
    })
}
