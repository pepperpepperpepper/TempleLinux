struct Gfx {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,

    fb_texture: wgpu::Texture,
    fb_size: wgpu::Extent3d,
}

impl Gfx {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("request adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let fb_size = wgpu::Extent3d {
            width: INTERNAL_W,
            height: INTERNAL_H,
            depth_or_array_layers: 1,
        };
        let fb_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("internal framebuffer"),
            size: fb_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let fb_view = fb_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fb sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fb bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fb bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&fb_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertices = [
            Vertex {
                position: [-1.0, -1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [1.0, -1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-1.0, 1.0],
                uv: [0.0, 0.0],
            },
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group,
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            fb_texture,
            fb_size,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn write_framebuffer(&self, rgba: &[u8]) {
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.fb_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(INTERNAL_W * 4),
                rows_per_image: Some(INTERNAL_H),
            },
            self.fb_size,
        );
    }

    fn render(&mut self, letterbox: Letterbox) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            pass.set_scissor_rect(
                letterbox.dest_x,
                letterbox.dest_y,
                letterbox.dest_w,
                letterbox.dest_h,
            );
            pass.set_viewport(
                letterbox.dest_x as f32,
                letterbox.dest_y as f32,
                letterbox.dest_w as f32,
                letterbox.dest_h as f32,
                0.0,
                1.0,
            );

            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl Vertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }
}

struct TempleAppSession {
    fb: memmap2::Mmap,
    width: u32,
    height: u32,
    cmd_tx: mpsc::Sender<protocol::Msg>,
    pending_present_ack_seq: Option<u32>,
}

#[derive(Debug, Clone)]
enum TestMode {
    DumpInitialFrame { png_path: PathBuf },
    DumpAfterFirstAppPresent { png_path: PathBuf },
    DumpAfterNAppsPresent { png_path: PathBuf, n: usize },
    DumpAfterNPresents { png_path: PathBuf, n: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestAppExit {
    Enter,
    CtrlQ,
    Esc,
    MouseLeft,
    None,
}

#[derive(Debug, Clone)]
struct TestState {
    mode: TestMode,
    pending_app_present: bool,
    did_dump: bool,
    exit_after_app_disconnect: bool,
    exit_now: bool,
    app_exit: TestAppExit,
    send_after_first_app_present: Vec<protocol::Msg>,
    did_send_after_first_app_present: bool,
    run_shell: Vec<String>,
    run_shell_idx: usize,
    run_shell_wait_for_app_count: Option<usize>,
    apps_presented: std::collections::BTreeSet<AppId>,
    presents_total: usize,
}

impl TestState {
    fn dump_initial_frame(png_path: PathBuf) -> Self {
        Self {
            mode: TestMode::DumpInitialFrame { png_path },
            pending_app_present: false,
            did_dump: false,
            exit_after_app_disconnect: false,
            exit_now: false,
            app_exit: TestAppExit::Enter,
            send_after_first_app_present: Vec::new(),
            did_send_after_first_app_present: false,
            run_shell: Vec::new(),
            run_shell_idx: 0,
            run_shell_wait_for_app_count: None,
            apps_presented: std::collections::BTreeSet::new(),
            presents_total: 0,
        }
    }

    fn dump_after_first_app_present(png_path: PathBuf, app_exit: TestAppExit) -> Self {
        Self {
            mode: TestMode::DumpAfterFirstAppPresent { png_path },
            pending_app_present: false,
            did_dump: false,
            exit_after_app_disconnect: false,
            exit_now: false,
            app_exit,
            send_after_first_app_present: Vec::new(),
            did_send_after_first_app_present: false,
            run_shell: Vec::new(),
            run_shell_idx: 0,
            run_shell_wait_for_app_count: None,
            apps_presented: std::collections::BTreeSet::new(),
            presents_total: 0,
        }
    }

    fn dump_after_n_apps_present(png_path: PathBuf, n: usize) -> Self {
        Self {
            mode: TestMode::DumpAfterNAppsPresent { png_path, n },
            pending_app_present: false,
            did_dump: false,
            exit_after_app_disconnect: false,
            exit_now: false,
            app_exit: TestAppExit::Enter,
            send_after_first_app_present: Vec::new(),
            did_send_after_first_app_present: false,
            run_shell: Vec::new(),
            run_shell_idx: 0,
            run_shell_wait_for_app_count: None,
            apps_presented: std::collections::BTreeSet::new(),
            presents_total: 0,
        }
    }

    fn dump_after_n_presents(png_path: PathBuf, n: usize, app_exit: TestAppExit) -> Self {
        Self {
            mode: TestMode::DumpAfterNPresents { png_path, n },
            pending_app_present: false,
            did_dump: false,
            exit_after_app_disconnect: false,
            exit_now: false,
            app_exit,
            send_after_first_app_present: Vec::new(),
            did_send_after_first_app_present: false,
            run_shell: Vec::new(),
            run_shell_idx: 0,
            run_shell_wait_for_app_count: None,
            apps_presented: std::collections::BTreeSet::new(),
            presents_total: 0,
        }
    }

    fn run_shell_done(&self) -> bool {
        self.run_shell_idx >= self.run_shell.len() && self.run_shell_wait_for_app_count.is_none()
    }

    fn on_app_present(&mut self, id: AppId) {
        if self.did_dump {
            return;
        }
        self.presents_total = self.presents_total.saturating_add(1);
        match &self.mode {
            TestMode::DumpAfterFirstAppPresent { .. } => {
                self.pending_app_present = true;
            }
            TestMode::DumpAfterNAppsPresent { n, .. } => {
                self.apps_presented.insert(id);
                if self.apps_presented.len() >= *n {
                    self.pending_app_present = true;
                }
            }
            TestMode::DumpAfterNPresents { n, .. } => {
                if self.presents_total >= *n {
                    self.pending_app_present = true;
                }
            }
            _ => {}
        }
    }

    fn on_app_disconnected(&mut self) {
        if self.exit_after_app_disconnect {
            self.exit_now = true;
        }
    }

    fn maybe_dump(
        &mut self,
        fb_rgba: &[u8],
        temple_app: Option<&TempleAppSession>,
    ) -> io::Result<()> {
        if self.did_dump {
            return Ok(());
        }

        if !self.run_shell_done() {
            return Ok(());
        }

        if !self.did_send_after_first_app_present
            && !self.send_after_first_app_present.is_empty()
            && self.presents_total >= 1
        {
            if let Some(sess) = temple_app {
                for msg in self.send_after_first_app_present.iter().copied() {
                    let _ = sess.cmd_tx.send(msg);
                }
                self.did_send_after_first_app_present = true;
            }
        }

        match &self.mode {
            TestMode::DumpInitialFrame { png_path } => {
                write_png_rgba(png_path, INTERNAL_W, INTERNAL_H, fb_rgba)?;
                self.did_dump = true;
                self.exit_now = true;
            }
            TestMode::DumpAfterFirstAppPresent { png_path } => {
                if !self.pending_app_present {
                    return Ok(());
                }
                write_png_rgba(png_path, INTERNAL_W, INTERNAL_H, fb_rgba)?;
                self.did_dump = true;
                self.pending_app_present = false;

                if let Some(sess) = temple_app {
                    match self.app_exit {
                        TestAppExit::Enter => {
                            // Demos frequently call PressAKey(). Send Enter to allow a graceful exit.
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ENTER, true));
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ENTER, false));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::CtrlQ => {
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_CONTROL, true));
                            let _ = sess.cmd_tx.send(protocol::Msg::key(b'q' as u32, true));
                            let _ = sess.cmd_tx.send(protocol::Msg::key(b'q' as u32, false));
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_CONTROL, false));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::Esc => {
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ESCAPE, true));
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ESCAPE, false));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::MouseLeft => {
                            let _ = sess.cmd_tx.send(protocol::Msg::mouse_button(
                                protocol::MOUSE_BUTTON_LEFT,
                                true,
                            ));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::None => {
                            self.exit_now = true;
                        }
                    }
                } else {
                    self.exit_now = true;
                }
            }
            TestMode::DumpAfterNAppsPresent { png_path, .. } => {
                if !self.pending_app_present {
                    return Ok(());
                }
                write_png_rgba(png_path, INTERNAL_W, INTERNAL_H, fb_rgba)?;
                self.did_dump = true;
                self.pending_app_present = false;
                self.exit_now = true;
            }
            TestMode::DumpAfterNPresents { png_path, .. } => {
                if !self.pending_app_present {
                    return Ok(());
                }
                write_png_rgba(png_path, INTERNAL_W, INTERNAL_H, fb_rgba)?;
                self.did_dump = true;
                self.pending_app_present = false;

                if let Some(sess) = temple_app {
                    match self.app_exit {
                        TestAppExit::Enter => {
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ENTER, true));
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ENTER, false));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::CtrlQ => {
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_CONTROL, true));
                            let _ = sess.cmd_tx.send(protocol::Msg::key(b'q' as u32, true));
                            let _ = sess.cmd_tx.send(protocol::Msg::key(b'q' as u32, false));
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_CONTROL, false));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::Esc => {
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ESCAPE, true));
                            let _ = sess
                                .cmd_tx
                                .send(protocol::Msg::key(protocol::KEY_ESCAPE, false));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::MouseLeft => {
                            let _ = sess.cmd_tx.send(protocol::Msg::mouse_button(
                                protocol::MOUSE_BUTTON_LEFT,
                                true,
                            ));
                            self.exit_after_app_disconnect = true;
                        }
                        TestAppExit::None => {
                            self.exit_now = true;
                        }
                    }
                } else {
                    self.exit_now = true;
                }
            }
        }

        Ok(())
    }
}
