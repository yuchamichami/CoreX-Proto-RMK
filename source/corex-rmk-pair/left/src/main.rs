#![no_std]
#![no_main]
use rmk::macros::rmk_peripheral;
#[rmk_peripheral(id = 0)]
mod keyboard {}
