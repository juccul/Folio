// Doing `if let` gives you nice scoping with passes/encoders
#![allow(irrefutable_let_patterns)]

use super::buffer_belt::{BufferBelt, BufferBeltDescriptor};
use super::resize::path_target_extent;
use super::{BladeAtlas, BladeContext};
use crate::{
    Background, BlurRegion, Bounds, DevicePixels, GpuSpecs, MonochromeSprite, Path, Point,
    PolychromeSprite, PrimitiveBatch, Quad, ScaledPixels, Scene, Shadow, Size, Underline,
};
use blade_graphics as gpu;
use bytemuck::{Pod, Zeroable};
#[cfg(target_os = "macos")]
use media::core_video::CVMetalTextureCache;
use std::sync::Arc;

const MAX_FRAME_TIME_MS: u32 = 10000;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalParams {
    viewport_size: [f32; 2],
    premultiplied_alpha: u32,
    pad: u32,
}

//Note: we can't use `Bounds` directly here because
// it doesn't implement Pod + Zeroable
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PodBounds {
    origin: [f32; 2],
    size: [f32; 2],
}

impl From<Bounds<ScaledPixels>> for PodBounds {
    fn from(bounds: Bounds<ScaledPixels>) -> Self {
        Self {
            origin: [bounds.origin.x.0, bounds.origin.y.0],
            size: [bounds.size.width.0, bounds.size.height.0],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SurfaceParams {
    bounds: PodBounds,
    content_mask: PodBounds,
}

#[derive(blade_macros::ShaderData)]
struct ShaderQuadsData {
    globals: GlobalParams,
    b_quads: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderShadowsData {
    globals: GlobalParams,
    b_shadows: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderPathRasterizationData {
    globals: GlobalParams,
    b_path_vertices: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderPathsData {
    globals: GlobalParams,
    t_sprite: gpu::TextureView,
    s_sprite: gpu::Sampler,
    b_path_sprites: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderUnderlinesData {
    globals: GlobalParams,
    b_underlines: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderMonoSpritesData {
    globals: GlobalParams,
    gamma_ratios: [f32; 4],
    grayscale_enhanced_contrast: f32,
    t_sprite: gpu::TextureView,
    s_sprite: gpu::Sampler,
    b_mono_sprites: gpu::BufferPiece,
}

#[derive(blade_macros::ShaderData)]
struct ShaderPolySpritesData {
    globals: GlobalParams,
    t_sprite: gpu::TextureView,
    s_sprite: gpu::Sampler,
    b_poly_sprites: gpu::BufferPiece,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurParams {
    source: [f32; 4],
    textures: [f32; 4],
    target_and_direction: [f32; 4],
    settings: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurComposite {
    bounds: [f32; 4],
    content_mask: [f32; 4],
    corner_radii: [f32; 4],
    source: [f32; 4],
    settings: [f32; 4],
}

#[derive(blade_macros::ShaderData)]
struct ShaderBlurData {
    blur_params: BlurParams,
    t_blur: gpu::TextureView,
    s_blur: gpu::Sampler,
}

#[derive(blade_macros::ShaderData)]
struct ShaderBlurCompositeData {
    globals: GlobalParams,
    blur_composite: BlurComposite,
    t_blur: gpu::TextureView,
    s_blur: gpu::Sampler,
}

#[derive(blade_macros::ShaderData)]
struct ShaderSurfacesData {
    globals: GlobalParams,
    surface_locals: SurfaceParams,
    t_y: gpu::TextureView,
    t_cb_cr: gpu::TextureView,
    s_surface: gpu::Sampler,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(C)]
struct PathSprite {
    bounds: Bounds<ScaledPixels>,
}

#[derive(Clone, Debug)]
#[repr(C)]
struct PathRasterizationVertex {
    xy_position: Point<ScaledPixels>,
    st_position: Point<f32>,
    color: Background,
    bounds: Bounds<ScaledPixels>,
}

struct BladePipelines {
    quads: gpu::RenderPipeline,
    shadows: gpu::RenderPipeline,
    path_rasterization: gpu::RenderPipeline,
    paths: gpu::RenderPipeline,
    underlines: gpu::RenderPipeline,
    mono_sprites: gpu::RenderPipeline,
    poly_sprites: gpu::RenderPipeline,
    surfaces: gpu::RenderPipeline,
    blur: gpu::RenderPipeline,
    blur_composite: gpu::RenderPipeline,
}

impl BladePipelines {
    fn new(gpu: &gpu::Context, surface_info: gpu::SurfaceInfo, path_sample_count: u32) -> Self {
        use gpu::ShaderData as _;

        log::info!(
            "Initializing Blade pipelines for surface {:?}",
            surface_info
        );
        let shader = gpu.create_shader(gpu::ShaderDesc {
            source: include_str!("shaders.wgsl"),
        });
        shader.check_struct_size::<GlobalParams>();
        shader.check_struct_size::<SurfaceParams>();
        shader.check_struct_size::<Quad>();
        shader.check_struct_size::<Shadow>();
        shader.check_struct_size::<PathRasterizationVertex>();
        shader.check_struct_size::<PathSprite>();
        shader.check_struct_size::<Underline>();
        shader.check_struct_size::<MonochromeSprite>();
        shader.check_struct_size::<PolychromeSprite>();
        shader.check_struct_size::<BlurParams>();
        shader.check_struct_size::<BlurComposite>();

        // See https://apoorvaj.io/alpha-compositing-opengl-blending-and-premultiplied-alpha/
        let blend_mode = match surface_info.alpha {
            gpu::AlphaMode::Ignored => gpu::BlendState::ALPHA_BLENDING,
            gpu::AlphaMode::PreMultiplied => gpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            gpu::AlphaMode::PostMultiplied => gpu::BlendState::ALPHA_BLENDING,
        };
        let color_targets = &[gpu::ColorTargetState {
            format: surface_info.format,
            blend: Some(blend_mode),
            write_mask: gpu::ColorWrites::default(),
        }];

        Self {
            quads: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "quads",
                data_layouts: &[&ShaderQuadsData::layout()],
                vertex: shader.at("vs_quad"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_quad")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            shadows: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "shadows",
                data_layouts: &[&ShaderShadowsData::layout()],
                vertex: shader.at("vs_shadow"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_shadow")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            path_rasterization: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "path_rasterization",
                data_layouts: &[&ShaderPathRasterizationData::layout()],
                vertex: shader.at("vs_path_rasterization"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_path_rasterization")),
                // The original implementation was using ADDITIVE blende mode,
                // I don't know why
                // color_targets: &[gpu::ColorTargetState {
                //     format: PATH_TEXTURE_FORMAT,
                //     blend: Some(gpu::BlendState::ADDITIVE),
                //     write_mask: gpu::ColorWrites::default(),
                // }],
                color_targets: &[gpu::ColorTargetState {
                    format: surface_info.format,
                    blend: Some(gpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: gpu::ColorWrites::default(),
                }],
                multisample_state: gpu::MultisampleState {
                    sample_count: path_sample_count,
                    ..Default::default()
                },
            }),
            paths: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "paths",
                data_layouts: &[&ShaderPathsData::layout()],
                vertex: shader.at("vs_path"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_path")),
                color_targets: &[gpu::ColorTargetState {
                    format: surface_info.format,
                    blend: Some(gpu::BlendState {
                        color: gpu::BlendComponent::OVER,
                        alpha: gpu::BlendComponent::ADDITIVE,
                    }),
                    write_mask: gpu::ColorWrites::default(),
                }],
                multisample_state: gpu::MultisampleState::default(),
            }),
            underlines: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "underlines",
                data_layouts: &[&ShaderUnderlinesData::layout()],
                vertex: shader.at("vs_underline"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_underline")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            mono_sprites: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "mono-sprites",
                data_layouts: &[&ShaderMonoSpritesData::layout()],
                vertex: shader.at("vs_mono_sprite"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_mono_sprite")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            poly_sprites: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "poly-sprites",
                data_layouts: &[&ShaderPolySpritesData::layout()],
                vertex: shader.at("vs_poly_sprite"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_poly_sprite")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            surfaces: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "surfaces",
                data_layouts: &[&ShaderSurfacesData::layout()],
                vertex: shader.at("vs_surface"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_surface")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
            blur: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "backdrop-blur",
                data_layouts: &[&ShaderBlurData::layout()],
                vertex: shader.at("vs_blur"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_blur")),
                color_targets: &[gpu::ColorTargetState {
                    format: surface_info.format,
                    blend: None,
                    write_mask: gpu::ColorWrites::default(),
                }],
                multisample_state: gpu::MultisampleState::default(),
            }),
            blur_composite: gpu.create_render_pipeline(gpu::RenderPipelineDesc {
                name: "backdrop-blur-composite",
                data_layouts: &[&ShaderBlurCompositeData::layout()],
                vertex: shader.at("vs_blur_composite"),
                vertex_fetches: &[],
                primitive: gpu::PrimitiveState {
                    topology: gpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                fragment: Some(shader.at("fs_blur_composite")),
                color_targets,
                multisample_state: gpu::MultisampleState::default(),
            }),
        }
    }

    fn destroy(&mut self, gpu: &gpu::Context) {
        gpu.destroy_render_pipeline(&mut self.quads);
        gpu.destroy_render_pipeline(&mut self.shadows);
        gpu.destroy_render_pipeline(&mut self.path_rasterization);
        gpu.destroy_render_pipeline(&mut self.paths);
        gpu.destroy_render_pipeline(&mut self.underlines);
        gpu.destroy_render_pipeline(&mut self.mono_sprites);
        gpu.destroy_render_pipeline(&mut self.poly_sprites);
        gpu.destroy_render_pipeline(&mut self.surfaces);
        gpu.destroy_render_pipeline(&mut self.blur);
        gpu.destroy_render_pipeline(&mut self.blur_composite);
    }
}

/// Scratch images for backdrop blur, allocated when a scene first blurs.
/// The backdrop is always reduced by at least half, so the blur images are
/// half the size of the window.
struct BlurTargets {
    viewport: gpu::Extent,
    capture: (gpu::Texture, gpu::TextureView),
    reduced: (gpu::Texture, gpu::TextureView),
    blurred: (gpu::Texture, gpu::TextureView),
    reduced_size: gpu::Extent,
    initialized: bool,
}

impl BlurTargets {
    fn new(gpu: &gpu::Context, format: gpu::TextureFormat, viewport: gpu::Extent) -> Self {
        let reduced_size = gpu::Extent {
            width: viewport.width.div_ceil(2).max(1),
            height: viewport.height.div_ceil(2).max(1),
            depth: 1,
        };
        let create = |name, size: gpu::Extent| {
            let texture = gpu.create_texture(gpu::TextureDesc {
                name,
                format,
                size,
                array_layer_count: 1,
                mip_level_count: 1,
                sample_count: 1,
                dimension: gpu::TextureDimension::D2,
                usage: gpu::TextureUsage::COPY
                    | gpu::TextureUsage::RESOURCE
                    | gpu::TextureUsage::TARGET,
                external: None,
            });
            let view = gpu.create_texture_view(
                texture,
                gpu::TextureViewDesc {
                    name,
                    format,
                    dimension: gpu::ViewDimension::D2,
                    subresources: &Default::default(),
                },
            );
            (texture, view)
        };
        Self {
            viewport,
            capture: create("backdrop capture", viewport),
            reduced: create("backdrop reduced", reduced_size),
            blurred: create("backdrop blurred", reduced_size),
            reduced_size,
            initialized: false,
        }
    }

    fn destroy(self, gpu: &gpu::Context) {
        for (texture, view) in [self.capture, self.reduced, self.blurred] {
            gpu.destroy_texture_view(view);
            gpu.destroy_texture(texture);
        }
    }
}


// Allocate these only for scenes containing vector paths. Ordinary chrome,
// glyphs and image sprites do not need full-window path rasterization targets.
struct PathTargets {
    size: gpu::Extent,
    texture: gpu::Texture,
    view: gpu::TextureView,
    msaa: Option<(gpu::Texture, gpu::TextureView)>,
}
impl PathTargets {
    fn new(
        gpu: &gpu::Context,
        format: gpu::TextureFormat,
        viewport: gpu::Extent,
        samples: u32,
    ) -> Self {
        let [width, height] = path_target_extent([viewport.width, viewport.height], [0, 0]);
        let (texture, view) = create_path_intermediate_texture(gpu, format, width, height);
        Self {
            size: gpu::Extent {
                width,
                height,
                depth: 1,
            },
            texture,
            view,
            msaa: create_msaa_texture_if_needed(gpu, format, width, height, samples),
        }
    }
    fn destroy(self, gpu: &gpu::Context) {
        gpu.destroy_texture_view(self.view);
        gpu.destroy_texture(self.texture);
        if let Some((texture, view)) = self.msaa {
            gpu.destroy_texture_view(view);
            gpu.destroy_texture(texture);
        }
    }
}

pub struct BladeSurfaceConfig {
    pub size: gpu::Extent,
    pub transparent: bool,
}

//Note: we could see some of these fields moved into `BladeContext`
// so that they are shared between windows. E.g. `pipelines`.
// But that is complicated by the fact that pipelines depend on
// the format and alpha mode.
pub struct BladeRenderer {
    gpu: Arc<gpu::Context>,
    surface: gpu::Surface,
    surface_config: gpu::SurfaceConfig,
    /// The part of the surface the window shows. The surface can be larger
    /// while the platform crops it (see [`Self::update_drawable_size_with_slack`]).
    drawable_size: gpu::Extent,
    command_encoder: gpu::CommandEncoder,
    last_sync_point: Option<gpu::SyncPoint>,
    pipelines: BladePipelines,
    instance_belt: BufferBelt,
    atlas: Arc<BladeAtlas>,
    atlas_sampler: gpu::Sampler,
    #[cfg(target_os = "macos")]
    core_video_texture_cache: CVMetalTextureCache,
    path_targets: Option<PathTargets>,
    blur_targets: Option<BlurTargets>,
    blur_sampler: gpu::Sampler,
    rendering_parameters: RenderingParameters,
}

impl BladeRenderer {
    pub fn new<I: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle>(
        context: &BladeContext,
        window: &I,
        config: BladeSurfaceConfig,
    ) -> anyhow::Result<Self> {
        let surface_config = gpu::SurfaceConfig {
            size: config.size,
            // Backdrop blur copies regions of the frame.
            usage: gpu::TextureUsage::TARGET | gpu::TextureUsage::COPY,
            display_sync: gpu::DisplaySync::Recent,
            color_space: gpu::ColorSpace::Srgb,
            allow_exclusive_full_screen: false,
            transparent: config.transparent,
        };
        let surface = context
            .gpu
            .create_surface_configured(window, surface_config)
            .map_err(|err| anyhow::anyhow!("Failed to create surface: {err:?}"))?;

        let command_encoder = context.gpu.create_command_encoder(gpu::CommandEncoderDesc {
            name: "main",
            buffer_count: 2,
        });
        let rendering_parameters = RenderingParameters::from_env(context);
        let pipelines = BladePipelines::new(
            &context.gpu,
            surface.info(),
            rendering_parameters.path_sample_count,
        );
        let instance_belt = BufferBelt::new(BufferBeltDescriptor {
            memory: gpu::Memory::Shared,
            min_chunk_size: 0x1000,
            alignment: 0x40, // Vulkan `minStorageBufferOffsetAlignment` on Intel Xe
        });
        let atlas = Arc::new(BladeAtlas::new(&context.gpu));
        let atlas_sampler = context.gpu.create_sampler(gpu::SamplerDesc {
            name: "path rasterization sampler",
            mag_filter: gpu::FilterMode::Linear,
            min_filter: gpu::FilterMode::Linear,
            ..Default::default()
        });
        let blur_sampler = context.gpu.create_sampler(gpu::SamplerDesc {
            name: "backdrop blur sampler",
            address_modes: [gpu::AddressMode::ClampToEdge; 3],
            mag_filter: gpu::FilterMode::Linear,
            min_filter: gpu::FilterMode::Linear,
            ..Default::default()
        });

        #[cfg(target_os = "macos")]
        let core_video_texture_cache = unsafe {
            CVMetalTextureCache::new(
                objc2::rc::Retained::as_ptr(&context.gpu.metal_device()) as *mut _
            )
            .unwrap()
        };

        Ok(Self {
            gpu: Arc::clone(&context.gpu),
            surface,
            surface_config,
            drawable_size: config.size,
            command_encoder,
            last_sync_point: None,
            pipelines,
            instance_belt,
            atlas,
            atlas_sampler,
            #[cfg(target_os = "macos")]
            core_video_texture_cache,
            path_targets: None,
            blur_targets: None,
            blur_sampler,
            rendering_parameters,
        })
    }

    pub(crate) fn wait_for_gpu(&mut self) {
        if let Some(last_sp) = self.last_sync_point.take() {
            Self::wait_for_sync_point(&self.gpu, &last_sp);
        }
    }

    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "x11"))]
    pub(crate) fn wait_for_last_frame_on_background(
        &self,
        executor: &crate::BackgroundExecutor,
    ) -> Option<crate::Task<()>> {
        let sync_point = self.last_sync_point.clone()?;
        let gpu = Arc::clone(&self.gpu);
        Some(executor.spawn(async move {
            Self::wait_for_sync_point(&gpu, &sync_point);
        }))
    }

    fn wait_for_sync_point(gpu: &gpu::Context, sync_point: &gpu::SyncPoint) {
        if !gpu.wait_for(sync_point, MAX_FRAME_TIME_MS) {
            log::error!("GPU hung");
            #[cfg(target_os = "linux")]
            if gpu.device_information().driver_name == "radv" {
                log::error!(
                    "there's a known bug with amdgpu/radv, try setting ZED_PATH_SAMPLE_COUNT=0 as a workaround"
                );
                log::error!(
                    "if that helps you're running into https://github.com/zed-industries/zed/issues/26143"
                );
            }
            log::error!("your device information is: {:?}", gpu.device_information());
            while !gpu.wait_for(sync_point, MAX_FRAME_TIME_MS) {}
        }
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        self.update_drawable_size_impl(size, false, false);
    }

    /// Like `update_drawable_size`, but with `slack` the surface keeps spare
    /// capacity so that small steps of an interactive resize reuse it
    /// instead of recreating the swapchain. The caller must then crop the
    /// presented image to [`Self::viewport_size`], which is drawn at its top
    /// left. Returns whether the surface was reallocated.
    #[cfg_attr(not(feature = "wayland"), allow(dead_code))]
    pub fn update_drawable_size_with_slack(
        &mut self,
        size: Size<DevicePixels>,
        slack: bool,
    ) -> bool {
        self.update_drawable_size_impl(size, false, slack)
    }

    /// Like `update_drawable_size` but skips the check that the size has changed. This is useful in
    /// cases like restoring a window from minimization where the size is the same but the
    /// renderer's swap chain needs to be recreated.
    #[cfg_attr(
        any(target_os = "macos", target_os = "linux", target_os = "freebsd"),
        allow(dead_code)
    )]
    pub fn update_drawable_size_even_if_unchanged(&mut self, size: Size<DevicePixels>) {
        self.update_drawable_size_impl(size, true, false);
    }

    fn update_drawable_size_impl(
        &mut self,
        size: Size<DevicePixels>,
        always_resize: bool,
        slack: bool,
    ) -> bool {
        self.drawable_size = gpu::Extent {
            width: size.width.0 as u32,
            height: size.height.0 as u32,
            depth: 1,
        };
        let gpu_size = if slack {
            let current = self.surface_config.size;
            let [width, height] = path_target_extent(
                [self.drawable_size.width, self.drawable_size.height],
                [current.width, current.height],
            );
            gpu::Extent {
                width,
                height,
                depth: 1,
            }
        } else {
            self.drawable_size
        };
        let reallocate = always_resize || gpu_size != self.surface_config.size;
        if reallocate {
            self.wait_for_gpu();
            self.surface_config.size = gpu_size;
            self.gpu
                .reconfigure_surface(&mut self.surface, self.surface_config);
            if let Some(targets) = &self.path_targets
                && path_target_extent(
                    [gpu_size.width, gpu_size.height],
                    [targets.size.width, targets.size.height],
                ) != [targets.size.width, targets.size.height]
            {
                // Recreate only if the next scene needs paths. Library-only
                // windows must not reserve full-window multisampled images.
                self.path_targets.take().unwrap().destroy(&self.gpu);
            }
            if let Some(targets) = self.blur_targets.take() {
                targets.destroy(&self.gpu);
            }
        }
        reallocate
    }

    pub fn update_transparency(&mut self, transparent: bool) {
        if transparent != self.surface_config.transparent {
            self.wait_for_gpu();
            self.surface_config.transparent = transparent;
            self.gpu
                .reconfigure_surface(&mut self.surface, self.surface_config);
            if let Some(targets) = self.path_targets.take() {
                targets.destroy(&self.gpu);
            }
            if let Some(targets) = self.blur_targets.take() {
                targets.destroy(&self.gpu);
            }
            self.pipelines.destroy(&self.gpu);
            self.pipelines = BladePipelines::new(
                &self.gpu,
                self.surface.info(),
                self.rendering_parameters.path_sample_count,
            );
        }
    }

    /// The size of the allocated surface, at least [`Self::viewport_size`].
    #[cfg_attr(not(feature = "wayland"), allow(dead_code))]
    pub fn surface_size(&self) -> gpu::Extent {
        self.surface_config.size
    }

    /// The size of the part of the surface the window shows.
    #[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
    pub fn viewport_size(&self) -> gpu::Extent {
        self.drawable_size
    }

    pub fn sprite_atlas(&self) -> &Arc<BladeAtlas> {
        &self.atlas
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub fn gpu_specs(&self) -> GpuSpecs {
        let info = self.gpu.device_information();

        GpuSpecs {
            is_software_emulated: info.is_software_emulated,
            device_name: info.device_name.clone(),
            driver_name: info.driver_name.clone(),
            driver_info: info.driver_info.clone(),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn layer(&self) -> metal::MetalLayer {
        unsafe { foreign_types::ForeignType::from_ptr(self.layer_ptr()) }
    }

    #[cfg(target_os = "macos")]
    pub fn layer_ptr(&self) -> *mut metal::CAMetalLayer {
        objc2::rc::Retained::as_ptr(&self.surface.metal_layer()) as *mut _
    }

    #[profiling::function]
    fn draw_paths_to_intermediate(
        &mut self,
        paths: &[Path<ScaledPixels>],
        width: f32,
        height: f32,
    ) {
        let targets = self
            .path_targets
            .as_ref()
            .expect("Path targets prepared before drawing");
        self.command_encoder.init_texture(targets.texture);
        if let Some(msaa_texture) = targets.msaa.map(|(texture, _)| texture) {
            self.command_encoder.init_texture(msaa_texture);
        }

        let target = if let Some(msaa_view) = targets.msaa.map(|(_, view)| view) {
            gpu::RenderTarget {
                view: msaa_view,
                init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                finish_op: gpu::FinishOp::ResolveTo(targets.view),
            }
        } else {
            gpu::RenderTarget {
                view: targets.view,
                init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                finish_op: gpu::FinishOp::Store,
            }
        };
        if let mut pass = self.command_encoder.render(
            "rasterize paths",
            gpu::RenderTargetSet {
                colors: &[target],
                depth_stencil: None,
            },
        ) {
            let globals = GlobalParams {
                viewport_size: [width, height],
                premultiplied_alpha: 0,
                pad: 0,
            };
            let mut encoder = pass.with(&self.pipelines.path_rasterization);

            let mut vertices = Vec::new();
            for path in paths {
                vertices.extend(path.vertices.iter().map(|v| PathRasterizationVertex {
                    xy_position: v.xy_position,
                    st_position: v.st_position,
                    color: path.color,
                    bounds: path.clipped_bounds(),
                }));
            }
            let vertex_buf = unsafe { self.instance_belt.alloc_typed(&vertices, &self.gpu) };
            encoder.bind(
                0,
                &ShaderPathRasterizationData {
                    globals,
                    b_path_vertices: vertex_buf,
                },
            );
            encoder.draw(0, vertices.len() as u32, 0, 1);
        }
    }

    /// Blur the captured backdrop of one element into `blur_targets.blurred`:
    /// copy, reduce, then blur horizontally and vertically. Must be called
    /// between render passes.
    fn blur_backdrop(&mut self, frame_texture: gpu::Texture, region: &BlurRegion, sigma: f32) {
        let targets = self.blur_targets.as_mut().unwrap();
        if !targets.initialized {
            self.command_encoder.init_texture(targets.capture.0);
            self.command_encoder.init_texture(targets.reduced.0);
            self.command_encoder.init_texture(targets.blurred.0);
            targets.initialized = true;
        }
        let (capture, reduced, blurred) = (targets.capture, targets.reduced, targets.blurred);
        let viewport = targets.viewport;
        let reduced_size = targets.reduced_size;
        if let mut transfer = self.command_encoder.transfer("backdrop capture") {
            transfer.copy_texture_to_texture(
                gpu::TexturePiece {
                    texture: frame_texture,
                    mip_level: 0,
                    array_layer: 0,
                    origin: [region.origin[0], region.origin[1], 0],
                },
                gpu::TexturePiece {
                    texture: capture.0,
                    mip_level: 0,
                    array_layer: 0,
                    origin: [0, 0, 0],
                },
                gpu::Extent {
                    width: region.size[0],
                    height: region.size[1],
                    depth: 1,
                },
            );
        }
        let scale = region.scale as f32;
        let reduced_extent = [region.reduced[0] as f32, region.reduced[1] as f32];
        let reduced_texture = [reduced_size.width as f32, reduced_size.height as f32];
        let passes = [
            (
                "backdrop reduce",
                capture.1,
                reduced.1,
                BlurParams {
                    source: [0., 0., region.size[0] as f32, region.size[1] as f32],
                    textures: [
                        viewport.width as f32,
                        viewport.height as f32,
                        reduced_texture[0],
                        reduced_texture[1],
                    ],
                    target_and_direction: [reduced_extent[0], reduced_extent[1], 0., 0.],
                    settings: [0., scale, 0., 0.],
                },
            ),
            (
                "backdrop blur x",
                reduced.1,
                blurred.1,
                BlurParams {
                    source: [0., 0., reduced_extent[0], reduced_extent[1]],
                    textures: [
                        reduced_texture[0],
                        reduced_texture[1],
                        reduced_texture[0],
                        reduced_texture[1],
                    ],
                    target_and_direction: [reduced_extent[0], reduced_extent[1], 1., 0.],
                    settings: [sigma / scale, 1., 0., 0.],
                },
            ),
            (
                "backdrop blur y",
                blurred.1,
                reduced.1,
                BlurParams {
                    source: [0., 0., reduced_extent[0], reduced_extent[1]],
                    textures: [
                        reduced_texture[0],
                        reduced_texture[1],
                        reduced_texture[0],
                        reduced_texture[1],
                    ],
                    target_and_direction: [reduced_extent[0], reduced_extent[1], 0., 1.],
                    settings: [sigma / scale, 1., 0., 0.],
                },
            ),
        ];
        for (label, source, target, params) in passes {
            if let mut pass = self.command_encoder.render(
                label,
                gpu::RenderTargetSet {
                    colors: &[gpu::RenderTarget {
                        view: target,
                        init_op: gpu::InitOp::DontCare,
                        finish_op: gpu::FinishOp::Store,
                    }],
                    depth_stencil: None,
                },
            ) {
                let mut encoder = pass.with(&self.pipelines.blur);
                encoder.bind(
                    0,
                    &ShaderBlurData {
                        blur_params: params,
                        t_blur: source,
                        s_blur: self.blur_sampler,
                    },
                );
                encoder.draw(0, 4, 0, 1);
            }
        }
    }

    pub fn destroy(&mut self) {
        self.wait_for_gpu();
        self.atlas.destroy();
        self.gpu.destroy_sampler(self.atlas_sampler);
        self.gpu.destroy_sampler(self.blur_sampler);
        if let Some(targets) = self.blur_targets.take() {
            targets.destroy(&self.gpu);
        }
        self.instance_belt.destroy(&self.gpu);
        self.gpu.destroy_command_encoder(&mut self.command_encoder);
        self.pipelines.destroy(&self.gpu);
        self.gpu.destroy_surface(&mut self.surface);
        if let Some(targets) = self.path_targets.take() {
            targets.destroy(&self.gpu);
        }
    }

    pub fn draw(&mut self, scene: &Scene) {
        // Resizing destroys the blur targets, so existing ones match the surface.
        if !scene.backdrop_blurs.is_empty() && self.blur_targets.is_none() {
            self.blur_targets = Some(BlurTargets::new(
                &self.gpu,
                self.surface.info().format,
                self.surface_config.size,
            ));
        }
        if !scene.paths.is_empty() && self.path_targets.is_none() {
            self.path_targets = Some(PathTargets::new(
                &self.gpu,
                self.surface.info().format,
                self.surface_config.size,
                self.rendering_parameters.path_sample_count,
            ));
        }
        self.command_encoder.start();
        self.atlas.before_frame(&mut self.command_encoder);

        let frame = {
            profiling::scope!("acquire frame");
            self.surface.acquire_frame()
        };
        self.command_encoder.init_texture(frame.texture());

        let globals = GlobalParams {
            viewport_size: [
                self.surface_config.size.width as f32,
                self.surface_config.size.height as f32,
            ],
            premultiplied_alpha: match self.surface.info().alpha {
                gpu::AlphaMode::Ignored | gpu::AlphaMode::PostMultiplied => 0,
                gpu::AlphaMode::PreMultiplied => 1,
            },
            pad: 0,
        };

        let mut pass = self.command_encoder.render(
            "main",
            gpu::RenderTargetSet {
                colors: &[gpu::RenderTarget {
                    view: frame.texture_view(),
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::TransparentBlack),
                    finish_op: gpu::FinishOp::Store,
                }],
                depth_stencil: None,
            },
        );

        profiling::scope!("render pass");
        for batch in scene.batches() {
            match batch {
                PrimitiveBatch::BackdropBlurs(blurs) => {
                    // Blur only what is shown, not spare surface capacity.
                    let viewport = self.drawable_size;
                    for blur in blurs {
                        let Some(region) = blur.region([viewport.width, viewport.height])
                        else {
                            continue;
                        };
                        drop(pass);
                        self.blur_backdrop(frame.texture(), &region, blur.blur_radius.0);
                        pass = self.command_encoder.render(
                            "main",
                            gpu::RenderTargetSet {
                                colors: &[gpu::RenderTarget {
                                    view: frame.texture_view(),
                                    init_op: gpu::InitOp::Load,
                                    finish_op: gpu::FinishOp::Store,
                                }],
                                depth_stencil: None,
                            },
                        );
                        let targets = self.blur_targets.as_ref().unwrap();
                        let mut encoder = pass.with(&self.pipelines.blur_composite);
                        let mask = blur.content_mask.bounds;
                        encoder.bind(
                            0,
                            &ShaderBlurCompositeData {
                                globals,
                                blur_composite: BlurComposite {
                                    bounds: [
                                        blur.bounds.origin.x.0,
                                        blur.bounds.origin.y.0,
                                        blur.bounds.size.width.0,
                                        blur.bounds.size.height.0,
                                    ],
                                    content_mask: [
                                        mask.origin.x.0,
                                        mask.origin.y.0,
                                        mask.size.width.0,
                                        mask.size.height.0,
                                    ],
                                    corner_radii: [
                                        blur.corner_radii.top_left.0,
                                        blur.corner_radii.top_right.0,
                                        blur.corner_radii.bottom_right.0,
                                        blur.corner_radii.bottom_left.0,
                                    ],
                                    source: [
                                        region.origin[0] as f32,
                                        region.origin[1] as f32,
                                        region.reduced[0] as f32,
                                        region.reduced[1] as f32,
                                    ],
                                    settings: [
                                        targets.reduced_size.width as f32,
                                        targets.reduced_size.height as f32,
                                        region.scale as f32,
                                        blur.opacity,
                                    ],
                                },
                                t_blur: targets.reduced.1,
                                s_blur: self.blur_sampler,
                            },
                        );
                        encoder.draw(0, 4, 0, 1);
                    }
                }
                PrimitiveBatch::Quads(quads) => {
                    let instance_buf = unsafe { self.instance_belt.alloc_typed(quads, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.quads);
                    encoder.bind(
                        0,
                        &ShaderQuadsData {
                            globals,
                            b_quads: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, quads.len() as u32);
                }
                PrimitiveBatch::Shadows(shadows) => {
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(shadows, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.shadows);
                    encoder.bind(
                        0,
                        &ShaderShadowsData {
                            globals,
                            b_shadows: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, shadows.len() as u32);
                }
                PrimitiveBatch::Paths(paths) => {
                    let Some(first_path) = paths.first() else {
                        continue;
                    };
                    drop(pass);
                    let targets = self.path_targets.as_ref().unwrap();
                    let (size, view) = (targets.size, targets.view);
                    self.draw_paths_to_intermediate(paths, size.width as f32, size.height as f32);
                    pass = self.command_encoder.render(
                        "main",
                        gpu::RenderTargetSet {
                            colors: &[gpu::RenderTarget {
                                view: frame.texture_view(),
                                init_op: gpu::InitOp::Load,
                                finish_op: gpu::FinishOp::Store,
                            }],
                            depth_stencil: None,
                        },
                    );
                    let mut encoder = pass.with(&self.pipelines.paths);
                    // When copying paths from the intermediate texture to the drawable,
                    // each pixel must only be copied once, in case of transparent paths.
                    //
                    // If all paths have the same draw order, then their bounds are all
                    // disjoint, so we can copy each path's bounds individually. If this
                    // batch combines different draw orders, we perform a single copy
                    // for a minimal spanning rect.
                    let sprites = if paths.last().unwrap().order == first_path.order {
                        paths
                            .iter()
                            .map(|path| PathSprite {
                                bounds: path.clipped_bounds(),
                            })
                            .collect()
                    } else {
                        let mut bounds = first_path.clipped_bounds();
                        for path in paths.iter().skip(1) {
                            bounds = bounds.union(&path.clipped_bounds());
                        }
                        vec![PathSprite { bounds }]
                    };
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(&sprites, &self.gpu) };
                    encoder.bind(
                        0,
                        &ShaderPathsData {
                            globals,
                            t_sprite: view,
                            s_sprite: self.atlas_sampler,
                            b_path_sprites: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, sprites.len() as u32);
                }
                PrimitiveBatch::Underlines(underlines) => {
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(underlines, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.underlines);
                    encoder.bind(
                        0,
                        &ShaderUnderlinesData {
                            globals,
                            b_underlines: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, underlines.len() as u32);
                }
                PrimitiveBatch::MonochromeSprites {
                    texture_id,
                    sprites,
                } => {
                    let tex_info = self.atlas.get_texture_info(texture_id);
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(sprites, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.mono_sprites);
                    encoder.bind(
                        0,
                        &ShaderMonoSpritesData {
                            globals,
                            gamma_ratios: self.rendering_parameters.gamma_ratios,
                            grayscale_enhanced_contrast: self
                                .rendering_parameters
                                .grayscale_enhanced_contrast,
                            t_sprite: tex_info.raw_view,
                            s_sprite: self.atlas_sampler,
                            b_mono_sprites: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, sprites.len() as u32);
                }
                PrimitiveBatch::PolychromeSprites {
                    texture_id,
                    sprites,
                } => {
                    let tex_info = self.atlas.get_texture_info(texture_id);
                    let instance_buf =
                        unsafe { self.instance_belt.alloc_typed(sprites, &self.gpu) };
                    let mut encoder = pass.with(&self.pipelines.poly_sprites);
                    encoder.bind(
                        0,
                        &ShaderPolySpritesData {
                            globals,
                            t_sprite: tex_info.raw_view,
                            s_sprite: self.atlas_sampler,
                            b_poly_sprites: instance_buf,
                        },
                    );
                    encoder.draw(0, 4, 0, sprites.len() as u32);
                }
                PrimitiveBatch::Surfaces(surfaces) => {
                    let mut _encoder = pass.with(&self.pipelines.surfaces);

                    for surface in surfaces {
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = surface;
                            continue;
                        };

                        #[cfg(target_os = "macos")]
                        {
                            let (t_y, t_cb_cr) = unsafe {
                                use core_foundation::base::TCFType as _;
                                use std::ptr;

                                assert_eq!(
                                        surface.image_buffer.get_pixel_format(),
                                        core_video::pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
                                    );

                                let y_texture = self
                                    .core_video_texture_cache
                                    .create_texture_from_image(
                                        surface.image_buffer.as_concrete_TypeRef(),
                                        ptr::null(),
                                        metal::MTLPixelFormat::R8Unorm,
                                        surface.image_buffer.get_width_of_plane(0),
                                        surface.image_buffer.get_height_of_plane(0),
                                        0,
                                    )
                                    .unwrap();
                                let cb_cr_texture = self
                                    .core_video_texture_cache
                                    .create_texture_from_image(
                                        surface.image_buffer.as_concrete_TypeRef(),
                                        ptr::null(),
                                        metal::MTLPixelFormat::RG8Unorm,
                                        surface.image_buffer.get_width_of_plane(1),
                                        surface.image_buffer.get_height_of_plane(1),
                                        1,
                                    )
                                    .unwrap();
                                (
                                    gpu::TextureView::from_metal_texture(
                                        &objc2::rc::Retained::retain(
                                            foreign_types::ForeignTypeRef::as_ptr(
                                                y_texture.as_texture_ref(),
                                            )
                                                as *mut objc2::runtime::ProtocolObject<
                                                    dyn objc2_metal::MTLTexture,
                                                >,
                                        )
                                        .unwrap(),
                                        gpu::TexelAspects::COLOR,
                                    ),
                                    gpu::TextureView::from_metal_texture(
                                        &objc2::rc::Retained::retain(
                                            foreign_types::ForeignTypeRef::as_ptr(
                                                cb_cr_texture.as_texture_ref(),
                                            )
                                                as *mut objc2::runtime::ProtocolObject<
                                                    dyn objc2_metal::MTLTexture,
                                                >,
                                        )
                                        .unwrap(),
                                        gpu::TexelAspects::COLOR,
                                    ),
                                )
                            };

                            _encoder.bind(
                                0,
                                &ShaderSurfacesData {
                                    globals,
                                    surface_locals: SurfaceParams {
                                        bounds: surface.bounds.into(),
                                        content_mask: surface.content_mask.bounds.into(),
                                    },
                                    t_y,
                                    t_cb_cr,
                                    s_surface: self.atlas_sampler,
                                },
                            );

                            _encoder.draw(0, 4, 0, 1);
                        }
                    }
                }
            }
        }
        drop(pass);

        self.command_encoder.present(frame);
        let sync_point = self.gpu.submit(&mut self.command_encoder);

        profiling::scope!("finish");
        self.instance_belt.flush(&sync_point, &self.gpu);
        self.atlas.after_frame(&sync_point);

        self.wait_for_gpu();
        self.atlas.reclaim_retired();
        self.last_sync_point = Some(sync_point);
    }
}

fn create_path_intermediate_texture(
    gpu: &gpu::Context,
    format: gpu::TextureFormat,
    width: u32,
    height: u32,
) -> (gpu::Texture, gpu::TextureView) {
    let texture = gpu.create_texture(gpu::TextureDesc {
        name: "path intermediate",
        format,
        size: gpu::Extent {
            width,
            height,
            depth: 1,
        },
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count: 1,
        dimension: gpu::TextureDimension::D2,
        usage: gpu::TextureUsage::COPY | gpu::TextureUsage::RESOURCE | gpu::TextureUsage::TARGET,
        external: None,
    });
    let texture_view = gpu.create_texture_view(
        texture,
        gpu::TextureViewDesc {
            name: "path intermediate view",
            format,
            dimension: gpu::ViewDimension::D2,
            subresources: &Default::default(),
        },
    );
    (texture, texture_view)
}

fn create_msaa_texture_if_needed(
    gpu: &gpu::Context,
    format: gpu::TextureFormat,
    width: u32,
    height: u32,
    sample_count: u32,
) -> Option<(gpu::Texture, gpu::TextureView)> {
    if sample_count <= 1 {
        return None;
    }
    let texture_msaa = gpu.create_texture(gpu::TextureDesc {
        name: "path intermediate msaa",
        format,
        size: gpu::Extent {
            width,
            height,
            depth: 1,
        },
        array_layer_count: 1,
        mip_level_count: 1,
        sample_count,
        dimension: gpu::TextureDimension::D2,
        usage: gpu::TextureUsage::TARGET,
        external: None,
    });
    let texture_view_msaa = gpu.create_texture_view(
        texture_msaa,
        gpu::TextureViewDesc {
            name: "path intermediate msaa view",
            format,
            dimension: gpu::ViewDimension::D2,
            subresources: &Default::default(),
        },
    );

    Some((texture_msaa, texture_view_msaa))
}

/// A set of parameters that can be set using a corresponding environment variable.
struct RenderingParameters {
    // Env var: ZED_PATH_SAMPLE_COUNT
    // workaround for https://github.com/zed-industries/zed/issues/26143
    path_sample_count: u32,

    // Env var: ZED_FONTS_GAMMA
    // Allowed range [1.0, 2.2], other values are clipped
    // Default: 1.8
    gamma_ratios: [f32; 4],
    // Env var: ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST
    // Allowed range: [0.0, ..), other values are clipped
    // Default: 1.0
    grayscale_enhanced_contrast: f32,
}

impl RenderingParameters {
    fn from_env(context: &BladeContext) -> Self {
        use std::env;

        let path_sample_count = env::var("ZED_PATH_SAMPLE_COUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| {
                [4, 2, 1]
                    .into_iter()
                    .find(|&n| (context.gpu.capabilities().sample_count_mask & n) != 0)
            })
            .unwrap_or(1);
        let gamma = env::var("ZED_FONTS_GAMMA")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.8_f32)
            .clamp(1.0, 2.2);
        let gamma_ratios = Self::get_gamma_ratios(gamma);
        let grayscale_enhanced_contrast = env::var("ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0_f32)
            .max(0.0);

        Self {
            path_sample_count,
            gamma_ratios,
            grayscale_enhanced_contrast,
        }
    }

    // Gamma ratios for brightening/darkening edges for better contrast
    // https://github.com/microsoft/terminal/blob/1283c0f5b99a2961673249fa77c6b986efb5086c/src/renderer/atlas/dwrite.cpp#L50
    fn get_gamma_ratios(gamma: f32) -> [f32; 4] {
        const GAMMA_INCORRECT_TARGET_RATIOS: [[f32; 4]; 13] = [
            [0.0000 / 4.0, 0.0000 / 4.0, 0.0000 / 4.0, 0.0000 / 4.0], // gamma = 1.0
            [0.0166 / 4.0, -0.0807 / 4.0, 0.2227 / 4.0, -0.0751 / 4.0], // gamma = 1.1
            [0.0350 / 4.0, -0.1760 / 4.0, 0.4325 / 4.0, -0.1370 / 4.0], // gamma = 1.2
            [0.0543 / 4.0, -0.2821 / 4.0, 0.6302 / 4.0, -0.1876 / 4.0], // gamma = 1.3
            [0.0739 / 4.0, -0.3963 / 4.0, 0.8167 / 4.0, -0.2287 / 4.0], // gamma = 1.4
            [0.0933 / 4.0, -0.5161 / 4.0, 0.9926 / 4.0, -0.2616 / 4.0], // gamma = 1.5
            [0.1121 / 4.0, -0.6395 / 4.0, 1.1588 / 4.0, -0.2877 / 4.0], // gamma = 1.6
            [0.1300 / 4.0, -0.7649 / 4.0, 1.3159 / 4.0, -0.3080 / 4.0], // gamma = 1.7
            [0.1469 / 4.0, -0.8911 / 4.0, 1.4644 / 4.0, -0.3234 / 4.0], // gamma = 1.8
            [0.1627 / 4.0, -1.0170 / 4.0, 1.6051 / 4.0, -0.3347 / 4.0], // gamma = 1.9
            [0.1773 / 4.0, -1.1420 / 4.0, 1.7385 / 4.0, -0.3426 / 4.0], // gamma = 2.0
            [0.1908 / 4.0, -1.2652 / 4.0, 1.8650 / 4.0, -0.3476 / 4.0], // gamma = 2.1
            [0.2031 / 4.0, -1.3864 / 4.0, 1.9851 / 4.0, -0.3501 / 4.0], // gamma = 2.2
        ];

        const NORM13: f32 = ((0x10000 as f64) / (255.0 * 255.0) * 4.0) as f32;
        const NORM24: f32 = ((0x100 as f64) / (255.0) * 4.0) as f32;

        let index = ((gamma * 10.0).round() as usize).clamp(10, 22) - 10;
        let ratios = GAMMA_INCORRECT_TARGET_RATIOS[index];

        [
            ratios[0] * NORM13,
            ratios[1] * NORM24,
            ratios[2] * NORM13,
            ratios[3] * NORM24,
        ]
    }
}
