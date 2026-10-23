use crate::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct GpuContext(pub(crate) Arc<Inner>);

pub(crate) struct Inner {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl GpuContext {
    pub async fn new() -> Result<Self> {
        Self::with_adapter(None).await
    }
    pub async fn with_adapter(name: Option<&str>) -> Result<Self> {
        let instance = wgpu::Instance::default();
        let adapter = if let Some(name) = name {
            #[cfg(not(target_arch = "wasm32"))]
            {
                instance
                    .enumerate_adapters(wgpu::Backends::all())
                    .await
                    .into_iter()
                    .find(|a| {
                        a.get_info()
                            .name
                            .to_lowercase()
                            .contains(&name.to_lowercase())
                    })
                    .ok_or_else(|| format!("no adapter matching {name:?}"))?
            }
            #[cfg(target_arch = "wasm32")]
            {
                return Err(format!(
                    "named adapter selection is unavailable in browsers: {name}"
                ));
            }
        } else {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .map_err(|e| e.to_string())?
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self(Arc::new(Inner {
            instance,
            adapter,
            device,
            queue,
        })))
    }
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.0.adapter.get_info()
    }
    pub fn limits(&self) -> wgpu::Limits {
        self.0.device.limits()
    }
    pub fn features(&self) -> wgpu::Features {
        self.0.device.features()
    }
    pub fn device(&self) -> &wgpu::Device {
        &self.0.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.0.queue
    }
    pub fn instance(&self) -> &wgpu::Instance {
        &self.0.instance
    }
    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
}
