use alloc::{boxed::Box, format, sync::Arc, vec, vec::Vec};

use jvm::Jvm;
use wipi_types::wipic::{WIPICIndirectPtr, WIPICWord};

use wie_backend::{AsyncCallable, Instant, System};
use wie_core_arm::{Allocator, ArmCore, HEAP_BASE, HEAP_SIZE};
use wie_util::{ByteRead, ByteWrite, Result, WieError, read_generic, write_generic};
use wie_wipi_c::{
    WIPICContext, WIPICMethodBody,
    api::{
        filesystem::SharedFilesystemState,
        graphics::{ContextLayout, ImageLayout},
        im::SharedImState,
        kernel::SharedKernelState,
        media::SharedMediaState,
        net::SharedNetworkState,
        serial::SharedSerialState,
        shared_buf::SharedSharedBufState,
    },
};

use super::daf::Resources;

/// The word in front of the data of an `MC_knlAlloc` block: the data pointer
/// the handle points at, and nothing else.
///
/// A title takes the data from the handle with a single load - 해적편's file
/// buffer is `*MC_knlAlloc(size)`, straight into `memset` - so the handle's
/// first word is the data's address and nothing is added to it. 메이플스토리
/// 도적편 goes the other way too: its `operator delete` hands `MC_knlFree` the
/// data less four, so the data starts right after that word. The block's size
/// is the allocator's to know.
const ALLOC_HEADER_SIZE: WIPICWord = 4;

#[derive(Clone)]
pub struct SktWIPICContext {
    pub core: ArmCore,
    pub system: System,
    pub jvm: Jvm,
    pub resources: Arc<Resources>,
    pub network_state: SharedNetworkState,
    pub serial_state: SharedSerialState,
    pub filesystem_state: SharedFilesystemState,
    pub shared_buf_state: SharedSharedBufState,
    pub im_state: SharedImState,
    pub media_state: SharedMediaState,
    pub kernel_state: SharedKernelState,
}

#[async_trait::async_trait]
impl WIPICContext for SktWIPICContext {
    /// The handset's `MC_GrpContext` is the whole struct, mask first - the
    /// layout KTF keeps too. See `ContextLayout`.
    fn graphics_context_layout(&self) -> ContextLayout {
        ContextLayout::Ktf
    }

    fn image_layout(&self) -> ImageLayout {
        ImageLayout::Inline
    }

    fn draw_image_keys_transparent_pixel(&self) -> bool {
        true
    }

    fn alloc_raw(&mut self, size: WIPICWord) -> Result<WIPICWord> {
        Allocator::alloc(&mut self.core, size)
    }

    fn alloc(&mut self, size: WIPICWord) -> Result<WIPICIndirectPtr> {
        let block = Allocator::alloc(&mut self.core, size + ALLOC_HEADER_SIZE)?;
        write_generic(&mut self.core, block, block + ALLOC_HEADER_SIZE)?;

        Ok(WIPICIndirectPtr(block))
    }

    fn free(&mut self, memory: WIPICIndirectPtr) -> Result<()> {
        // Freeing null, or anything that is not a block from the heap, is
        // nothing, as `free(NULL)` is - see the LGT context for the titles
        // that free stray words.
        if memory.0 < HEAP_BASE || memory.0 >= HEAP_BASE + HEAP_SIZE {
            return Ok(());
        }

        Allocator::free_unsized(&mut self.core, memory.0)
    }

    fn free_raw(&mut self, address: WIPICWord, size: WIPICWord) -> Result<()> {
        Allocator::free(&mut self.core, address, size)
    }

    fn free_raw_unsized(&mut self, address: WIPICWord) -> Result<()> {
        Allocator::free_unsized(&mut self.core, address)
    }

    fn raw_alloc_size(&self, address: WIPICWord) -> Result<WIPICWord> {
        Allocator::allocation_size(&self.core, address)
    }

    fn data_ptr(&self, memory: WIPICIndirectPtr) -> Result<WIPICWord> {
        read_generic(&self.core, memory.0)
    }

    fn handle_of(&self, data: WIPICWord) -> WIPICWord {
        data - ALLOC_HEADER_SIZE
    }

    fn system(&mut self) -> &mut System {
        &mut self.system
    }

    fn network_state(&self) -> SharedNetworkState {
        self.network_state.clone()
    }

    fn serial_state(&self) -> SharedSerialState {
        self.serial_state.clone()
    }

    fn filesystem_state(&self) -> SharedFilesystemState {
        self.filesystem_state.clone()
    }

    fn shared_buf_state(&self) -> SharedSharedBufState {
        self.shared_buf_state.clone()
    }

    fn im_state(&self) -> SharedImState {
        self.im_state.clone()
    }

    fn media_state(&self) -> SharedMediaState {
        self.media_state.clone()
    }

    fn kernel_state(&self) -> SharedKernelState {
        self.kernel_state.clone()
    }

    async fn call_function(&mut self, address: WIPICWord, args: &[WIPICWord]) -> Result<WIPICWord> {
        self.core.run_function(address, args).await
    }

    fn spawn(&mut self, callback: WIPICMethodBody) -> Result<()> {
        struct SpawnProxy {
            context: SktWIPICContext,
            callback: WIPICMethodBody,
        }

        impl AsyncCallable<Result<()>> for SpawnProxy {
            async fn call(mut self) -> Result<()> {
                self.context.jvm.attach_thread(None).await.unwrap();
                self.callback.call(&mut self.context, Box::new([])).await?;
                self.context.jvm.detach_thread().unwrap();

                Ok(())
            }
        }

        self.system.spawn(SpawnProxy {
            context: self.clone(),
            callback,
        });

        Ok(())
    }

    /// The title's own files come out of its executable; anything else - a
    /// save it wrote, a file the archive carries beside the executable - from
    /// the filesystem.
    async fn get_resource_size(&self, name: &str) -> Result<Option<usize>> {
        if let Some(data) = self.resources.get(name) {
            return Ok(Some(data.len()));
        }

        Ok(self.system.filesystem().size(name).await)
    }

    async fn read_resource(&self, name: &str) -> Result<Vec<u8>> {
        if let Some(data) = self.resources.get(name) {
            return Ok(data.to_vec());
        }

        let Some(size) = self.system.filesystem().size(name).await else {
            return Err(WieError::FatalError(format!("Missing resource: {name}")));
        };
        let mut data = vec![0; size];
        let read = self.system.filesystem().read(name, 0, size, &mut data).await.unwrap_or(0);
        data.truncate(read);

        Ok(data)
    }

    fn set_timer(&mut self, id: WIPICWord, due: Instant, callback: WIPICMethodBody) {
        let context = self.clone();

        self.system().event_queue().push_timer(id, due, move || {
            let mut context = context.clone();

            async move {
                callback.call(&mut context, Box::new([])).await?;
                Ok(())
            }
        })
    }

    fn unset_timer(&mut self, id: WIPICWord) {
        self.system().event_queue().cancel_timer(id);
    }
}

impl ByteRead for SktWIPICContext {
    fn read_bytes(&self, address: WIPICWord, result: &mut [u8]) -> wie_util::Result<usize> {
        self.core.read_bytes(address, result)
    }
}

impl ByteWrite for SktWIPICContext {
    fn write_bytes(&mut self, address: WIPICWord, data: &[u8]) -> wie_util::Result<()> {
        self.core.write_bytes(address, data)
    }
}
