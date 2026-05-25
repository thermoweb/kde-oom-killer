#[allow(dead_code)]
pub enum WindowRequest {
    Settings,
    History,
}

#[cfg(feature = "sni")]
mod sni;
#[cfg(feature = "sni")]
pub use sni::start;

#[cfg(not(feature = "sni"))]
mod fallback;
#[cfg(not(feature = "sni"))]
pub use fallback::start;
