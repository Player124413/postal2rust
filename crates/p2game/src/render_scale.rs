//! Renders the 3D world at a fraction of the window's pixels and stretches it to the window.
//!
//! On the phone the window is 2652x1200 physical pixels and the Adreno GPU is the bottleneck: the
//! world's fill cost scales with the pixel count. The world cameras (order < 100) draw into an
//! image of `scale` x the window size; a display camera (order -10) shows that image full-screen
//! on its own render layer. The 2D canvas (order 100), the menus and the touch controls stay at
//! full resolution, so text and the HUD stay sharp.
//!
//! `P2GAME_RENDER_SCALE` sets the factor (1.0 turns the feature off). On Android it defaults to 0.6.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, ClearColorConfig, ImageRenderTarget, RenderTarget};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::render::render_resource::{Extent3d, TextureFormat};

/// The layer the display sprite lives on; only the display camera draws it.
pub const DISPLAY_LAYER: usize = 31;

/// Cameras with an order below this draw the world; the 2D canvas starts at 100.
const WORLD_MAX_ORDER: isize = 100;

pub struct RenderScalePlugin;

impl Plugin for RenderScalePlugin {
    fn build(&self, app: &mut App) {
        let scale = scale_factor();
        if scale >= 1.0 {
            return;
        }
        println!("render: world drawn at {:.0}% of the window and stretched", scale * 100.0);
        app.insert_resource(WorldScale(scale)).add_systems(Update, (ensure_display, retarget_world_cameras, sync_world_size).chain());
    }
}

/// The world render factor (1.0 = off).
#[derive(Resource)]
struct WorldScale(f32);

/// The world image and the window size it was made for (physical pixels).
#[derive(Resource)]
struct WorldImage {
    handle: Handle<Image>,
    size: UVec2,
}

#[derive(Component)]
struct DisplaySprite;

fn scale_factor() -> f32 {
    let default = if cfg!(target_os = "android") { 0.6 } else { 1.0 };
    std::env::var("P2GAME_RENDER_SCALE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .map(|v| v.clamp(0.25, 1.0))
        .unwrap_or(default)
}

fn world_size(window: &Window, scale: f32) -> UVec2 {
    UVec2::new(
        ((window.physical_width() as f32) * scale).round().max(1.0) as u32,
        ((window.physical_height() as f32) * scale).round().max(1.0) as u32,
    )
}

/// Creates the world image and the display camera + sprite once the window exists.
fn ensure_display(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    scale: Res<WorldScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    existing: Option<Res<WorldImage>>,
) {
    if existing.is_some() {
        return;
    }
    let Ok(window) = windows.single() else { return };
    if window.physical_width() == 0 || window.physical_height() == 0 {
        return;
    }
    let size = world_size(window, scale.0);
    let mut image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    image.sampler = ImageSampler::linear();
    let handle = images.add(image);
    commands.insert_resource(WorldImage { handle: handle.clone(), size: UVec2::new(window.physical_width(), window.physical_height()) });
    commands.spawn((
        Camera2d,
        Camera { order: -10, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderLayers::layer(DISPLAY_LAYER),
        if cfg!(target_os = "android") { bevy::render::view::Msaa::Off } else { bevy::render::view::Msaa::default() },
    ));
    commands.spawn((
        DisplaySprite,
        Sprite { custom_size: Some(Vec2::new(window.width(), window.height())), ..Sprite::from_image(handle) },
        RenderLayers::layer(DISPLAY_LAYER),
    ));
}

/// Points every world camera at the world image (they were spawned with the window as target).
fn retarget_world_cameras(world: Option<Res<WorldImage>>, mut cams: Query<(&Camera, &mut RenderTarget), With<Camera3d>>) {
    let Some(world) = world else { return };
    for (cam, mut target) in &mut cams {
        if cam.order >= WORLD_MAX_ORDER || !matches!(*target, RenderTarget::Window(_)) {
            continue;
        }
        *target = RenderTarget::Image(ImageRenderTarget { handle: world.handle.clone(), scale_factor: 1.0 });
    }
}

/// Keeps the world image and the display sprite matched to the window size (rotation, resize).
fn sync_world_size(
    world: Option<ResMut<WorldImage>>,
    mut images: ResMut<Assets<Image>>,
    scale: Res<WorldScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sprites: Query<&mut Sprite, With<DisplaySprite>>,
) {
    let Some(mut world) = world else { return };
    let Ok(window) = windows.single() else { return };
    let physical = UVec2::new(window.physical_width(), window.physical_height());
    if physical == world.size || physical.x == 0 || physical.y == 0 {
        return;
    }
    world.size = physical;
    let size = world_size(window, scale.0);
    if let Some(mut image) = images.get_mut(&world.handle) {
        image.resize(Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 });
    }
    for mut sprite in &mut sprites {
        sprite.custom_size = Some(Vec2::new(window.width(), window.height()));
    }
}
