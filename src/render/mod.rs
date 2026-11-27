//! Renderer plumbing: framebuffer, camera and scene.

pub mod camera;
pub mod canvas;
pub mod scene;

pub use camera::Camera;
pub use canvas::{rgb, Canvas, Color};
pub use scene::{Scene, SceneOptions};
