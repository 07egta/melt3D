//! Headless egui-wgpu renderer for SolidCraft's ignored screenshot preview.
//! This draws the real egui paint jobs and SolidCraft viewport callback.

use egui_wgpu::wgpu;
use std::sync::Arc;

pub(super) struct GpuPreview {
    pub render_state: egui_wgpu::RenderState,
    color: wgpu::Texture,
    depth: wgpu::Texture,
    readback: wgpu::Buffer,
}

impl GpuPreview {
    pub fn new() -> Option<Self> {
        pollster::block_on(async {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
            if let Some(backends) = wgpu::Backends::from_env() {
                desc.backends = backends;
            }
            let instance = wgpu::Instance::new(desc);
            let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.ok()?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_features: adapter.features() & wgpu::Features::DEPTH32FLOAT_STENCIL8,
                    required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
                    ..Default::default()
                })
                .await
                .ok()?;
            let format = wgpu::TextureFormat::Rgba8Unorm;
            let depth_format = egui_wgpu::depth_format_from_bits(24, 8).expect("24 bit depth with stencil is supported");
            let renderer = egui_wgpu::Renderer::new(
                &device,
                format,
                egui_wgpu::RendererOptions { msaa_samples: 1, depth_stencil_format: Some(depth_format), ..Default::default() },
            );
            let msaa_supported = egui_wgpu::supported_msaa_samples(&adapter, &device, format, Some(depth_format));
            let render_state = egui_wgpu::RenderState {
                adapter: adapter.clone(),
                available_adapters: vec![adapter],
                instance,
                device,
                queue,
                target_format: format,
                msaa_samples: 1,
                msaa_supported,
                renderer: Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
                surface_config: egui_wgpu::SurfaceConfig::HIGH_THROUGHPUT,
                device_lost: Default::default(),
            };
            let color = render_state.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("SolidCraft preview"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let readback = render_state.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("SolidCraft preview readback"),
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let depth = render_state.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("SolidCraft preview depth"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: depth_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            Some(Self { render_state, color, depth, readback })
        })
    }

    pub fn render(&mut self, ctx: &egui::Context, output: &egui::FullOutput, size: egui::Vec2, ppp: f32) -> Vec<[f32; 3]> {
        let [width, height] = [(size.x * ppp).ceil() as u32, (size.y * ppp).ceil() as u32];
        self.color = self.render_state.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SolidCraft preview frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.render_state.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = self.color.create_view(&Default::default());
        let depth_format = egui_wgpu::depth_format_from_bits(24, 8).expect("24 bit depth with stencil is supported");
        self.depth = self.render_state.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SolidCraft preview depth frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = self.depth.create_view(&Default::default());
        let row_bytes = width * 4;
        let padded_row_bytes = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        self.readback = self.render_state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SolidCraft preview readback"),
            size: u64::from(padded_row_bytes) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let jobs = ctx.tessellate(output.shapes.clone(), ppp);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [width, height], pixels_per_point: ppp };
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        let mut renderer = self.render_state.renderer.write();
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        let extra = renderer.update_buffers(device, queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("SolidCraft preview UI"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.2, g: 0.2, b: 0.22, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Store }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }
        encoder.copy_texture_to_buffer(
            self.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_row_bytes), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        queue.submit(extra.into_iter().chain([encoder.finish()]));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("wait for SolidCraft preview render");
        let slice = self.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).expect("map SolidCraft preview pixels");
        let mapped = slice.get_mapped_range().expect("read SolidCraft preview pixels");
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height as usize {
            let row = &mapped[y * padded_row_bytes as usize..y * padded_row_bytes as usize + row_bytes as usize];
            pixels.extend(row.as_chunks::<4>().0.iter().map(|px| [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0]));
        }
        drop(mapped);
        self.readback.unmap();
        for id in &output.textures_delta.free {
            renderer.free_texture(id);
        }
        pixels
    }
}
