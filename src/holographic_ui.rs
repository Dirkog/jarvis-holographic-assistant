//! Голографический интерфейс JARVIS v2.0
//! wgpu + Vulkan — прозрачные окна, шейдеры, 60 FPS
//! Text rendering: готов к wgpu_text (требует шрифт в assets/fonts/)

use crate::{JarvisState, SystemMetrics, UICommand};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, error, warn};
use winit::{
    event::{Event, WindowEvent, ElementState, VirtualKeyCode, KeyboardInput},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{WindowBuilder, WindowLevel},
};
use std::time::Instant;

/// Uniform data — 16-byte aligned для WGSL
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    time: f32,
    resolution_x: f32,
    resolution_y: f32,
    _padding: f32,
}

pub fn run_holographic_ui(
    _state: Arc<tokio::sync::RwLock<JarvisState>>,
    mut ui_cmd_rx: mpsc::Receiver<UICommand>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("[UI] Инициализация голографического интерфейса v2.0...");
    info!("[UI] AMD Ryzen 7 5825U + Vega 8 — Vulkan backend");

    let event_loop = EventLoopBuilder::new().build();

    let window = WindowBuilder::new()
        .with_title("J.A.R.V.I.S.")
        .with_inner_size(winit::dpi::LogicalSize::new(1920.0, 1080.0))
        .with_transparent(true)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_resizable(false)
        .build(&event_loop)?;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        dx12_shader_compiler: wgpu::Dx12Compiler::default(),
    });

    let surface = unsafe { instance.create_surface(&window) }?;

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    })).ok_or("GPU адаптер не найден")?;

    let adapter_info = adapter.get_info();
    info!("[UI] GPU: {} | Backend: {:?} | Driver: {}",
        adapter_info.name, adapter_info.backend, adapter_info.driver);

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
            label: Some("JARVIS Device"),
        },
        None,
    ))?;

    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps.formats.iter()
        .copied()
        .find(|f| f.is_srgb())
        .or_else(|| surface_caps.formats.first().copied())
        .ok_or("Нет доступных форматов surface")?;

    let alpha_mode = if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
        wgpu::CompositeAlphaMode::PostMultiplied
    } else if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
        wgpu::CompositeAlphaMode::PreMultiplied
    } else {
        wgpu::CompositeAlphaMode::Opaque
    };

    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: window.inner_size().width.max(1),
        height: window.inner_size().height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode,
        view_formats: vec![],
    };

    surface.configure(&device, &config);

    let uniforms = Uniforms {
        time: 0.0,
        resolution_x: config.width as f32,
        resolution_y: config.height as f32,
        _padding: 0.0,
    };

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Time Uniform"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Uniform Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Uniform Bind Group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Hologram Shader"),
        source: wgpu::ShaderSource::Wgsl(HOLOGRAM_SHADER.into()),
    });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Hologram Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    let mut metrics = SystemMetrics::default();
    let mut gesture_text = String::from("Ожидание...");
    let mut command_text = String::from("Готов к работе");
    let mut last_speech = String::new();
    let start_time = Instant::now();

    info!("[UI] Голографический интерфейс v2.0 запущен");
    info!("[UI] Горячие клавиши: F1=Point F2=OpenPalm F3=Pinch F4=Fist F5=SwipeLeft F6=SwipeRight F7=Metrics ESC=Exit");
    info!("[UI] Для text rendering: положите DejaVuSans.ttf в assets/fonts/ и добавьте wgpu_text");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        let elapsed = start_time.elapsed().as_secs_f32();
        let window_size = window.inner_size();
        let uniforms = Uniforms {
            time: elapsed,
            resolution_x: window_size.width.max(1) as f32,
            resolution_y: window_size.height.max(1) as f32,
            _padding: 0.0,
        };
        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        while let Ok(cmd) = ui_cmd_rx.try_recv() {
            match cmd {
                UICommand::UpdateMetrics(m) => metrics = m,
                UICommand::UpdateGesture(g) => gesture_text = g,
                UICommand::UpdateCommand(c) => command_text = c,
                UICommand::Speak(text) => last_speech = text,
                UICommand::Exit => *control_flow = ControlFlow::Exit,
            }
        }

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        config.width = size.width;
                        config.height = size.height;
                        surface.configure(&device, &config);
                    }
                }
                WindowEvent::KeyboardInput {
                    input: KeyboardInput { state: ElementState::Pressed, virtual_keycode: Some(key), .. },
                    ..
                } => {
                    match key {
                        VirtualKeyCode::Escape => *control_flow = ControlFlow::Exit,
                        VirtualKeyCode::F1 => { gesture_text = "POINT: Указание".to_string(); }
                        VirtualKeyCode::F2 => { gesture_text = "OPEN_PALM: Пауза".to_string(); }
                        VirtualKeyCode::F3 => { gesture_text = "PINCH: Клик".to_string(); }
                        VirtualKeyCode::F4 => { gesture_text = "CLOSED_FIST: Захват".to_string(); }
                        VirtualKeyCode::F5 => { gesture_text = "SWIPE_LEFT: Предыдущий стол".to_string(); }
                        VirtualKeyCode::F6 => { gesture_text = "SWIPE_RIGHT: Следующий стол".to_string(); }
                        VirtualKeyCode::F7 => {
                            command_text = format!(
                                "CPU: {:.0}% | RAM: {:.1}/{:.1}GB | GPU: {:.0}% @ {:.0}°C | NET: ↑{:.1} ↓{:.1}",
                                metrics.cpu_usage, metrics.ram_used_gb, metrics.ram_total_gb,
                                metrics.gpu_usage, metrics.gpu_temp,
                                metrics.network_up, metrics.network_down
                            );
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            Event::RedrawRequested(_) => {
                let frame = match surface.get_current_texture() {
                    Ok(f) => f,
                    Err(wgpu::SurfaceError::Lost) => {
                        surface.configure(&device, &config);
                        return;
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        error!("[UI] GPU out of memory!");
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    Err(e) => {
                        warn!("[UI] Surface error: {:?}", e);
                        return;
                    }
                };

                let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Hologram Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    render_pass.set_pipeline(&render_pipeline);
                    render_pass.set_bind_group(0, &bind_group, &[]);
                    render_pass.draw(0..3, 0..1);
                }

                queue.submit(std::iter::once(encoder.finish()));
                frame.present();
            }
            Event::MainEventsCleared => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}

const HOLOGRAM_SHADER: &str = r#"
struct Uniforms {
    time: f32,
    resolution_x: f32,
    resolution_y: f32,
    _padding: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(in_vertex_index) - 1) * 0.5;
    let y = f32(i32(in_vertex_index & 1u) * 2 - 1) * 0.5;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let resolution = vec2<f32>(uniforms.resolution_x, uniforms.resolution_y);
    let uv = frag_coord.xy / resolution;
    let center = vec2<f32>(0.5, 0.5);
    let dist = length(uv - center);
    let time = uniforms.time;

    let jarvis_blue = vec3<f32>(0.0, 0.65, 1.0);
    let jarvis_cyan = vec3<f32>(0.0, 0.9, 0.85);
    let jarvis_glow = vec3<f32>(0.3, 0.8, 1.0);

    let ring_speed = 1.5;
    let rings = sin(dist * 40.0 - time * ring_speed) * 0.5 + 0.5;
    let ring_intensity = smoothstep(0.45, 0.0, dist) * rings * 0.4;

    let scan_y = fract(time * 0.15);
    let scanline = smoothstep(0.003, 0.0, abs(uv.y - scan_y)) * 0.4;

    let circle_outer = smoothstep(0.25, 0.23, dist);
    let circle_inner = smoothstep(0.23, 0.21, dist);
    let circle = circle_outer - circle_inner;

    let core_pulse = sin(time * 2.0) * 0.02 + 0.08;
    let core = smoothstep(core_pulse, core_pulse - 0.02, dist) * 0.8;

    let angle = atan2(uv.y - 0.5, uv.x - 0.5);
    let arc = sin(angle * 3.0 + time * 1.5) * 0.5 + 0.5;
    let arc_ring = smoothstep(0.18, 0.17, dist) * smoothstep(0.15, 0.16, dist) * arc * 0.5;

    let deco_ring1 = smoothstep(0.32, 0.31, dist) * smoothstep(0.29, 0.30, dist) *
        sin(angle * 6.0 - time * 0.5) * 0.3;

    var color = vec3<f32>(0.0);
    color += jarvis_blue * ring_intensity;
    color += jarvis_cyan * circle * 0.6;
    color += jarvis_glow * core;
    color += jarvis_blue * arc_ring;
    color += jarvis_cyan * deco_ring1;
    color += vec3<f32>(0.5, 1.0, 1.0) * scanline;

    let noise = fract(sin(dot(uv * 1000.0 + time * 10.0, vec2<f32>(12.9898, 78.233))) * 43758.5453);
    color += noise * 0.03;

    let vignette = 1.0 - smoothstep(0.2, 0.6, dist);
    color *= vignette;

    let pulse = sin(time * 1.5) * 0.05 + 0.95;
    color *= pulse;

    let alpha = max(ring_intensity, circle) * 0.85 + core * 0.9 + scanline * 0.5 + arc_ring * 0.7 + deco_ring1 * 0.5;

    return vec4<f32>(color, alpha * 0.9);
}
"#;
