//! Matter OTA Requestor and OTA Provider state machines.

use heapless::String;

use crate::{Error, Result};

/// Maximum OTA software-version string length.
pub const MAX_VERSION_STRING: usize = 64;
/// Maximum provider URI length.
pub const MAX_OTA_URI: usize = 128;

/// Verified OTA image metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtaImage {
    /// Vendor identifier.
    pub vendor_id: u16,
    /// Product identifier.
    pub product_id: u16,
    /// Monotonic software version.
    pub software_version: u32,
    /// Human-readable version.
    pub software_version_string: String<MAX_VERSION_STRING>,
    /// Total payload size.
    pub image_size: u64,
    /// SHA-256 digest of the complete image.
    pub digest: [u8; 32],
}

impl OtaImage {
    /// Create image metadata.
    pub fn new(
        vendor_id: u16,
        product_id: u16,
        software_version: u32,
        software_version_string: &str,
        image_size: u64,
        digest: [u8; 32],
    ) -> Result<Self> {
        if vendor_id == 0 || product_id == 0 || software_version == 0 || image_size == 0 {
            return Err(Error::InvalidArgument);
        }
        let mut version = String::new();
        version
            .push_str(software_version_string)
            .map_err(|_| Error::Capacity)?;
        if version.is_empty() {
            return Err(Error::InvalidArgument);
        }
        Ok(Self {
            vendor_id,
            product_id,
            software_version,
            software_version_string: version,
            image_size,
            digest,
        })
    }
}

/// Persistent OTA requestor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OtaRequestorState {
    /// No update is active.
    Idle,
    /// Provider discovery or image query is active.
    Querying,
    /// Image blocks are being written to the inactive slot.
    Downloading {
        /// Image metadata.
        image: OtaImage,
        /// Next required byte offset.
        next_offset: u64,
    },
    /// Full-image verification is active.
    Verifying {
        /// Image metadata.
        image: OtaImage,
    },
    /// A verified image can be activated.
    ReadyToApply {
        /// Image metadata.
        image: OtaImage,
    },
    /// Boot control selected the pending image.
    Applying {
        /// Image metadata.
        image: OtaImage,
    },
    /// The last update failed and was aborted or rolled back.
    Failed,
}

/// Inactive image-slot and boot-control backend.
pub trait OtaSlot {
    /// Prepare the inactive slot for a new image.
    async fn begin(&mut self, image: &OtaImage) -> Result<()>;
    /// Write one sequential image block.
    async fn write(&mut self, offset: u64, data: &[u8]) -> Result<()>;
    /// Flush all image bytes.
    async fn finish(&mut self) -> Result<()>;
    /// Mark the inactive image as pending for the next boot.
    async fn mark_pending(&mut self, image: &OtaImage) -> Result<()>;
    /// Confirm the running image after health checks pass.
    async fn confirm_running(&mut self) -> Result<()>;
    /// Abort a download and erase incomplete image data.
    async fn abort(&mut self) -> Result<()>;
    /// Select the previous image after a failed health check.
    async fn rollback(&mut self) -> Result<()>;
}

/// Hardware-backed or software OTA image verifier.
pub trait OtaVerifier<S: OtaSlot> {
    /// Verify digest, signature, vendor/product identity, and image format in
    /// the inactive slot.
    async fn verify(&mut self, slot: &mut S, image: &OtaImage) -> Result<()>;
}

/// OTA Requestor controller with anti-rollback checks.
pub struct OtaRequestor<S, V> {
    slot: S,
    verifier: V,
    current_version: u32,
    minimum_version: u32,
    state: OtaRequestorState,
}

impl<S, V> OtaRequestor<S, V>
where
    S: OtaSlot,
    V: OtaVerifier<S>,
{
    /// Create a requestor.
    pub const fn new(slot: S, verifier: V, current_version: u32, minimum_version: u32) -> Self {
        Self {
            slot,
            verifier,
            current_version,
            minimum_version,
            state: OtaRequestorState::Idle,
        }
    }

    /// Return the persistent state.
    #[must_use]
    pub const fn state(&self) -> &OtaRequestorState {
        &self.state
    }

    /// Enter provider query state.
    pub fn start_query(&mut self) -> Result<()> {
        if self.state != OtaRequestorState::Idle {
            return Err(Error::Busy);
        }
        self.state = OtaRequestorState::Querying;
        Ok(())
    }

    /// Accept an offered image and prepare the inactive slot.
    pub async fn accept_image(&mut self, image: OtaImage) -> Result<()> {
        if self.state != OtaRequestorState::Querying {
            return Err(Error::InvalidState);
        }
        if image.software_version <= self.current_version
            || image.software_version < self.minimum_version
        {
            self.state = OtaRequestorState::Failed;
            return Err(Error::Verification);
        }
        if let Err(error) = self.slot.begin(&image).await {
            self.state = OtaRequestorState::Failed;
            return Err(error);
        }
        self.state = OtaRequestorState::Downloading {
            image,
            next_offset: 0,
        };
        Ok(())
    }

    /// Write the next sequential BDX image block.
    pub async fn write_block(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        let (image_size, expected_offset) = match &self.state {
            OtaRequestorState::Downloading { image, next_offset } => {
                (image.image_size, *next_offset)
            }
            _ => return Err(Error::InvalidState),
        };
        if data.is_empty() || offset != expected_offset {
            return Err(Error::InvalidArgument);
        }
        let next_offset = offset
            .checked_add(u64::try_from(data.len()).map_err(|_| Error::Capacity)?)
            .ok_or(Error::Capacity)?;
        if next_offset > image_size {
            return Err(Error::InvalidArgument);
        }
        self.slot.write(offset, data).await?;
        if let OtaRequestorState::Downloading {
            next_offset: stored,
            ..
        } = &mut self.state
        {
            *stored = next_offset;
        }
        Ok(())
    }

    /// Flush and verify a complete image.
    pub async fn finish_download(&mut self) -> Result<()> {
        let image = match &self.state {
            OtaRequestorState::Downloading { image, next_offset }
                if *next_offset == image.image_size =>
            {
                image.clone()
            }
            OtaRequestorState::Downloading { .. } => return Err(Error::InvalidState),
            _ => return Err(Error::InvalidState),
        };
        self.slot.finish().await?;
        self.state = OtaRequestorState::Verifying {
            image: image.clone(),
        };
        if self.verifier.verify(&mut self.slot, &image).await.is_err() {
            let _abort_result = self.slot.abort().await;
            self.state = OtaRequestorState::Failed;
            return Err(Error::Verification);
        }
        self.state = OtaRequestorState::ReadyToApply { image };
        Ok(())
    }

    /// Mark the verified image for activation.
    pub async fn apply(&mut self) -> Result<()> {
        let image = match &self.state {
            OtaRequestorState::ReadyToApply { image } => image.clone(),
            _ => return Err(Error::InvalidState),
        };
        self.slot.mark_pending(&image).await?;
        self.state = OtaRequestorState::Applying { image };
        Ok(())
    }

    /// Confirm the running image after application health checks pass.
    pub async fn confirm(&mut self) -> Result<()> {
        let version = match &self.state {
            OtaRequestorState::Applying { image } => image.software_version,
            _ => return Err(Error::InvalidState),
        };
        self.slot.confirm_running().await?;
        self.current_version = version;
        self.state = OtaRequestorState::Idle;
        Ok(())
    }

    /// Roll back a pending image after a failed boot health check.
    pub async fn rollback(&mut self) -> Result<()> {
        if !matches!(self.state, OtaRequestorState::Applying { .. }) {
            return Err(Error::InvalidState);
        }
        self.slot.rollback().await?;
        self.state = OtaRequestorState::Failed;
        Ok(())
    }

    /// Abort any non-applied transfer.
    pub async fn abort(&mut self) -> Result<()> {
        if matches!(self.state, OtaRequestorState::Applying { .. }) {
            return Err(Error::InvalidState);
        }
        self.slot.abort().await?;
        self.state = OtaRequestorState::Idle;
        Ok(())
    }

    /// Split the requestor into platform components.
    #[must_use]
    pub fn into_parts(self) -> (S, V) {
        (self.slot, self.verifier)
    }
}

/// Result of an OTA Provider query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OtaQueryResult {
    /// No applicable image exists.
    NotAvailable,
    /// An image is available.
    Available(OtaImage),
    /// The requestor must retry after this delay.
    Busy {
        /// Delay in seconds.
        retry_after_seconds: u32,
    },
}

/// OTA Provider backend.
pub trait OtaProviderBackend {
    /// Select an image for a requestor.
    async fn query(
        &mut self,
        vendor_id: u16,
        product_id: u16,
        current_version: u32,
    ) -> Result<OtaQueryResult>;
    /// Read one image block at an exact offset.
    async fn read_block(
        &mut self,
        image: &OtaImage,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize>;
}

/// Validating OTA Provider facade.
pub struct OtaProvider<B> {
    backend: B,
    maximum_block_size: usize,
}

impl<B> OtaProvider<B>
where
    B: OtaProviderBackend,
{
    /// Create a provider.
    pub fn new(backend: B, maximum_block_size: usize) -> Result<Self> {
        if maximum_block_size == 0 {
            return Err(Error::InvalidArgument);
        }
        Ok(Self {
            backend,
            maximum_block_size,
        })
    }

    /// Query for an update.
    pub async fn query(
        &mut self,
        vendor_id: u16,
        product_id: u16,
        current_version: u32,
    ) -> Result<OtaQueryResult> {
        self.backend
            .query(vendor_id, product_id, current_version)
            .await
    }

    /// Read a bounded image block.
    pub async fn read_block(
        &mut self,
        image: &OtaImage,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize> {
        if output.is_empty() || output.len() > self.maximum_block_size || offset >= image.image_size
        {
            return Err(Error::InvalidArgument);
        }
        let remaining = image.image_size - offset;
        let allowed =
            usize::try_from(remaining.min(output.len() as u64)).map_err(|_| Error::Capacity)?;
        let written = self
            .backend
            .read_block(image, offset, &mut output[..allowed])
            .await?;
        if written == 0 || written > allowed {
            return Err(Error::InvalidState);
        }
        Ok(written)
    }

    /// Consume the provider.
    #[must_use]
    pub fn into_inner(self) -> B {
        self.backend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(version: u32, size: u64) -> Result<OtaImage> {
        OtaImage::new(1, 2, version, "test", size, [0xA5; 32])
    }

    #[test]
    fn metadata_rejects_empty_or_zero_values() {
        assert_eq!(
            OtaImage::new(0, 2, 3, "v", 1, [0; 32]),
            Err(Error::InvalidArgument)
        );
        assert_eq!(
            OtaImage::new(1, 2, 3, "", 1, [0; 32]),
            Err(Error::InvalidArgument)
        );
        assert!(image(3, 10).is_ok());
    }
}
