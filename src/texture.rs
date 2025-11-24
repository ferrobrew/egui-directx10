// This file contains implementations inspired by or derived from the following
// sources:
// - https://github.com/ohchase/egui-directx/blob/master/egui-directx11/src/texture.rs
//
// Here I would express my gratitude for their contributions to the Rust
// community. Their work served as a valuable reference and inspiration for this
// project.
//
// Nekomaru, March 2024

use std::{collections::HashMap, mem};

use egui::{Color32, ImageData, TextureId, TexturesDelta};

use windows::{
    core::Result,
    Win32::Graphics::{Direct3D10::*, Dxgi::Common::*},
};

struct ManagedTexture {
    tex: ID3D10Texture2D,
    srv: ID3D10ShaderResourceView,
    pixels: Vec<Color32>,
    width: usize,
}

enum Texture {
    /// A texture managed by egui (created from ImageData)
    Managed(ManagedTexture),
    /// A user-provided texture (registered from an existing shader resource view)
    User { srv: ID3D10ShaderResourceView },
}

impl Texture {
    pub fn is_managed(&self) -> bool {
        matches!(self, Texture::Managed(_))
    }

    pub fn is_user(&self) -> bool {
        matches!(self, Texture::User { .. })
    }
}

pub struct TexturePool {
    device: ID3D10Device,
    pool: HashMap<TextureId, Texture>,
    next_user_texture_id: u64,
}

impl TexturePool {
    pub fn new(device: &ID3D10Device) -> Self {
        Self {
            device: device.clone(),
            pool: HashMap::new(),
            next_user_texture_id: 0,
        }
    }

    pub fn get_srv(&self, tid: TextureId) -> Option<ID3D10ShaderResourceView> {
        self.pool.get(&tid).map(|t| match t {
            Texture::Managed(managed) => managed.srv.clone(),
            Texture::User { srv } => srv.clone(),
        })
    }

    /// Register a user-provided shader resource view and get a TextureId for it.
    /// This TextureId can be used in egui to reference this texture.
    ///
    /// The returned TextureId will be unique and won't conflict with egui's managed textures.
    pub fn register_user_texture(
        &mut self,
        srv: ID3D10ShaderResourceView,
    ) -> TextureId {
        let id = TextureId::User(self.next_user_texture_id);
        self.next_user_texture_id += 1;
        self.pool.insert(id, Texture::User { srv });
        id
    }

    /// Unregister a user texture by its TextureId.
    /// Returns true if the texture was found and removed, false otherwise.
    pub fn unregister_user_texture(&mut self, tid: TextureId) -> bool {
        if self.pool.get(&tid).is_some_and(|t| t.is_user()) {
            self.pool.remove(&tid);
            true
        } else {
            false
        }
    }

    pub fn update(
        &mut self,
        ctx: &ID3D10Device,
        delta: TexturesDelta,
    ) -> Result<()> {
        for (tid, delta) in delta.set {
            if delta.is_whole()
                && delta.image.width() > 0
                && delta.image.height() > 0
            {
                self.pool.insert(
                    tid,
                    Self::create_managed_texture(&self.device, delta.image)?,
                );
                // the old texture is returned and dropped here, freeing
                // all its gpu resource.
            } else if let Some(tex) =
                self.pool.get_mut(&tid).filter(|t| t.is_managed())
            {
                Self::update_partial(
                    ctx,
                    tex,
                    delta.image,
                    delta.pos.unwrap(),
                )?;
            } else {
                log::warn!(
                    "egui wants to update a non-existing texture {tid:?}. this request will be ignored."
                );
            }
        }
        for tid in delta.free {
            if self.pool.get(&tid).is_some_and(|t| t.is_managed()) {
                self.pool.remove(&tid);
            }
        }
        Ok(())
    }

    fn update_partial(
        ctx: &ID3D10Device,
        old: &mut Texture,
        image: ImageData,
        [nx, ny]: [usize; 2],
    ) -> Result<()> {
        let Texture::Managed(old) = old else {
            log::warn!(
                "attempted to partially update a user texture, which is not supported"
            );
            return Ok(());
        };

        match image {
            ImageData::Color(f) => {
                let row_pitch = f.width() * 4; // 4 bytes per pixel
                let mut update_data = vec![0u8; f.height() * row_pitch];

                for y in 0..f.height() {
                    for x in 0..f.width() {
                        let frac = y * f.width() + x;
                        let whole = (ny + y) * old.width + nx + x;
                        let dst_idx = y * row_pitch + x * 4;

                        // Update old.pixels
                        old.pixels[whole] = f.pixels[frac];

                        // Update update_data
                        let color_array = f.pixels[frac].to_array();
                        update_data[dst_idx..dst_idx + 4]
                            .copy_from_slice(&color_array);
                    }
                }

                let subresource_data = D3D10_BOX {
                    left: nx as u32,
                    top: ny as u32,
                    front: 0,
                    right: (nx + f.width()) as u32,
                    bottom: (ny + f.height()) as u32,
                    back: 1,
                };

                unsafe {
                    ctx.UpdateSubresource(
                        &old.tex,
                        0,
                        Some(&subresource_data),
                        update_data.as_ptr() as _,
                        row_pitch as u32,
                        0,
                    );
                }
            },
        }
        Ok(())
    }

    fn create_managed_texture(
        device: &ID3D10Device,
        data: ImageData,
    ) -> Result<Texture> {
        let width = data.width();

        let pixels = match &data {
            ImageData::Color(c) => c.pixels.clone(),
        };

        let desc = D3D10_TEXTURE2D_DESC {
            Width: data.width() as _,
            Height: data.height() as _,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D10_USAGE_DYNAMIC,
            BindFlags: D3D10_BIND_SHADER_RESOURCE.0 as _,
            CPUAccessFlags: D3D10_CPU_ACCESS_WRITE.0 as _,
            ..Default::default()
        };

        let subresource_data = D3D10_SUBRESOURCE_DATA {
            pSysMem: pixels.as_ptr() as _,
            SysMemPitch: (width * mem::size_of::<Color32>()) as u32,
            SysMemSlicePitch: 0,
        };

        let tex =
            unsafe { device.CreateTexture2D(&desc, Some(&subresource_data)) }?;

        let mut srv = None;
        unsafe { device.CreateShaderResourceView(&tex, None, Some(&mut srv)) }?;
        let srv = srv.unwrap();

        Ok(Texture::Managed(ManagedTexture {
            tex,
            srv,
            width,
            pixels,
        }))
    }
}
