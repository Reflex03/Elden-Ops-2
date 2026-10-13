//! Elden Ops: Black Ops II Zombies rounds in Elden Ring.

pub mod game;
pub mod sheets;

#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod er;
#[cfg(windows)]
mod hud;
#[cfg(windows)]
mod melty_test;
#[cfg(windows)]
mod plugin;
