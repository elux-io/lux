use std::f32::consts::TAU;

use glam::{Mat4, Quat, Vec3};
use lux::App as _;
use lux_derive::HotReload;
use rand::Rng;
use wgpu::{include_spirv, util::DeviceExt};
use winit::{dpi::PhysicalSize, window::Window};

#[derive(HotReload)]
pub struct App {
    render_device: RenderDevice,
    size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    depth_texture: wgpu::Texture,

    time: f32,

    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    cubes: Vec<Cube>,
    cube_mesh: GpuMesh,
    cubes_instance_buffer: wgpu::Buffer,
}

impl lux::App for App {
    fn new(window: &Window) -> Self {
        let size = window.inner_size();
        let render_device = pollster::block_on(RenderDevice::new(&window));
        let device = &render_device.device;

        let shader =
            device.create_shader_module(include_spirv!(concat!(env!("OUT_DIR"), "/shader.spv")));

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Buffer"),
            size: 16 * 4,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "main",
                buffers: &[VertexData::desc(), InstanceData::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_device.config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let mut rng = rand::thread_rng();
        let mut cubes: Vec<_> = (0..10)
            .map(|_| Cube {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                velocity: Vec3::ZERO,
                target_position: Vec3::ZERO,
                rotation_delta: Quat::from_rotation_x(rng.gen_range(0.01..0.03))
                    * Quat::from_rotation_y(rng.gen_range(0.01..0.03)),
            })
            .collect();

        compute_target_positions(&mut cubes);

        let instance_data_size = 16 * 4;
        let cubes_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cubes Instance Buffer"),
            size: (cubes.len() * instance_data_size) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let cube_mesh = GpuMesh::new(&build_cube_mesh(), &device);
        let depth_texture = create_depth_texture(&device, size.width, size.height);

        Self {
            render_device,
            size,
            render_pipeline,
            depth_texture,
            time: 0.0,
            cubes,
            cube_mesh,
            cubes_instance_buffer,
            camera_buffer,
            camera_bind_group,
        }
    }

    fn update(&mut self) {
        // Update camera
        let camera_pos = Vec3::new(
            (self.time * 0.5).cos() * 10.0,
            2.0,
            (self.time * 0.5).sin() * 10.0,
        );

        let view_matrix = Mat4::look_at_rh(camera_pos, Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
        let aspect_ratio = self.size.width as f32 / self.size.height as f32;
        let proj_matrix = Mat4::perspective_rh(60.0f32.to_radians(), aspect_ratio, 0.1, 100.0);
        let view_proj = (proj_matrix * view_matrix).to_cols_array();
        self.render_device.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&view_proj));

        // Update cubes
        for cube in &mut self.cubes {
            let acceleration = (cube.target_position - cube.position) * 0.001 - cube.velocity * 0.03;
            cube.velocity += acceleration;
            cube.position += cube.velocity;
            cube.rotation *= cube.rotation_delta;
        }

        let cubes_instance_buffer_data: Vec<[f32; 16]> = self.cubes
            .iter()
            .map(|cube| (Mat4::from_translation(cube.position) * Mat4::from_quat(cube.rotation)).to_cols_array())
            .collect();

        self.render_device.queue.write_buffer(
            &self.cubes_instance_buffer,
            0,
            bytemuck::cast_slice(&cubes_instance_buffer_data),
        );

        self.time += 1.0 / 60.0;

        self.render();
    }

    fn on_resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.size = PhysicalSize::new(width, height);
            self.render_device.config.width = width;
            self.render_device.config.height = height;
            self.render_device.surface.configure(&self.render_device.device, &self.render_device.config);
            self.depth_texture = create_depth_texture(&self.render_device.device, width, height);
        }
    }
}

impl App {
    fn render(&mut self) {
        let output = self.render_device.surface.get_current_texture().unwrap();

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.render_device
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        {
            let depth_texture_view = self
                .depth_texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.005,
                            g: 0.005,
                            b: 0.005,
                            a: 1.0,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);

            render_pass.set_vertex_buffer(0, self.cube_mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.cube_mesh.index_buffer.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            render_pass.set_vertex_buffer(1, self.cubes_instance_buffer.slice(..));

            render_pass.draw_indexed(0..self.cube_mesh.index_count, 0, 0..self.cubes.len() as _);
        }

        self.render_device.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

#[derive(Copy, Clone)]
struct Cube {
    position: Vec3,
    rotation: Quat,
    velocity: Vec3,
    target_position: Vec3,
    rotation_delta: Quat,
}

fn compute_target_positions(cubes: &mut Vec<Cube>) {
    let angular_step = TAU / cubes.len() as f32;
    let circle_radius = 4.5;

    for (i, cube) in cubes.iter_mut().enumerate() {
        let angle = i as f32 * angular_step;

        cube.target_position.x = angle.cos() * circle_radius;
        cube.target_position.y = 0.0;
        cube.target_position.z = angle.sin() * circle_radius;
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct VertexData {
    position: [f32; 3],
    normal: [f32; 3],
}

impl VertexData {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VertexData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    model_matrix: [f32; 16],
}

impl InstanceData {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 8]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 12]>() as wgpu::BufferAddress,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

struct Mesh {
    vertices: Vec<VertexData>,
    indices: Vec<u32>,
}

fn build_cube_mesh() -> Mesh {
    let s = 0.5;

    Mesh {
        #[rustfmt::skip]
        vertices: vec![
            // +x face
            VertexData { position: [s, s, s], normal: [1.0, 0.0, 0.0] },
            VertexData { position: [s, -s, s], normal: [1.0, 0.0, 0.0] },
            VertexData { position: [s, -s, -s], normal: [1.0, 0.0, 0.0] },
            VertexData { position: [s, s, -s], normal: [1.0, 0.0, 0.0] },

            // -x face
            VertexData { position: [-s, s, -s], normal: [-1.0, 0.0, 0.0] },
            VertexData { position: [-s, -s, -s], normal: [-1.0, 0.0, 0.0] },
            VertexData { position: [-s, -s, s], normal: [-1.0, 0.0, 0.0] },
            VertexData { position: [-s, s, s], normal: [-1.0, 0.0, 0.0] },

            // +y face
            VertexData { position: [-s, s, -s], normal: [0.0, 1.0, 0.0] },
            VertexData { position: [-s, s, s], normal: [0.0, 1.0, 0.0] },
            VertexData { position: [s, s, s], normal: [0.0, 1.0, 0.0] },
            VertexData { position: [s, s, -s], normal: [0.0, 1.0, 0.0] },

            // -y face
            VertexData { position: [-s, -s, s], normal: [0.0, -1.0, 0.0] },
            VertexData { position: [-s, -s, -s], normal: [0.0, -1.0, 0.0] },
            VertexData { position: [s, -s, -s], normal: [0.0, -1.0, 0.0] },
            VertexData { position: [s, -s, s], normal: [0.0, -1.0, 0.0] },

            // +z face
            VertexData { position: [-s, s, s], normal: [0.0, 0.0, 1.0] },
            VertexData { position: [-s, -s, s], normal: [0.0, 0.0, 1.0] },
            VertexData { position: [s, -s, s], normal: [0.0, 0.0, 1.0] },
            VertexData { position: [s, s, s], normal: [0.0, 0.0, 1.0] },

            // -z face
            VertexData { position: [-s, -s, -s], normal: [0.0, 0.0, -1.0] },
            VertexData { position: [-s, s, -s], normal: [0.0, 0.0, -1.0] },
            VertexData { position: [s, s, -s], normal: [0.0, 0.0, -1.0] },
            VertexData { position: [s, -s, -s], normal: [0.0, 0.0, -1.0] },
        ],

        #[rustfmt::skip]
        indices: vec![
            // +x triangles
            0, 1, 3,  1, 2, 3,

            // -x triangles
            4, 5, 7,  5, 6, 7,

            // +y triangles
            8, 9, 11,  9, 10, 11,

            // -y triangles
            12, 13, 15,  13, 14, 15,

            // +z triangles
            16, 17, 19,  17, 18, 19,

            // -z triangles
            20, 21, 23,  21, 22, 23,
        ],
    }
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl GpuMesh {
    fn new(mesh: &Mesh, device: &wgpu::Device) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("VertexData Buffer"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
        }
    }
}

struct RenderDevice {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl RenderDevice {
    async fn new(window: &Window) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });

        let surface = unsafe { instance.create_surface(window) }.unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
        }
    }
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
