use alloc::{boxed::Box, vec, vec::Vec};

use wipi_types::wipic::{WIPICIndirectPtr, WIPICWord};

use wie_backend::{Instant, System};
use wie_util::{ByteRead, ByteWrite, Result};

use crate::{
    WIPICMethodBody,
    api::{
        filesystem::SharedFilesystemState, graphics::ContextLayout, im::SharedImState, kernel::SharedKernelState, net::SharedNetworkState,
        serial::SharedSerialState, shared_buf::SharedSharedBufState,
    },
    method::{ParamConverter, ResultConverter},
};

#[async_trait::async_trait]
pub trait WIPICContext: ByteRead + ByteWrite + Send + Sync {
    fn alloc_raw(&mut self, size: WIPICWord) -> Result<WIPICWord>;
    fn alloc(&mut self, size: WIPICWord) -> Result<WIPICIndirectPtr>;
    fn free(&mut self, memory: WIPICIndirectPtr) -> Result<()>;
    fn free_raw(&mut self, address: WIPICWord, size: WIPICWord) -> Result<()>;
    fn free_raw_unsized(&mut self, address: WIPICWord) -> Result<()>;
    fn raw_alloc_size(&self, address: WIPICWord) -> Result<WIPICWord>;
    fn data_ptr(&self, memory: WIPICIndirectPtr) -> Result<WIPICWord>;
    async fn call_function(&mut self, address: WIPICWord, args: &[WIPICWord]) -> Result<WIPICWord>;
    fn system(&mut self) -> &mut System;
    fn network_state(&self) -> SharedNetworkState;
    fn serial_state(&self) -> SharedSerialState;
    fn filesystem_state(&self) -> SharedFilesystemState;
    fn shared_buf_state(&self) -> SharedSharedBufState;
    fn im_state(&self) -> SharedImState;
    fn kernel_state(&self) -> SharedKernelState;
    fn spawn(&mut self, callback: WIPICMethodBody) -> Result<()>;
    async fn get_resource_size(&self, name: &str) -> Result<Option<usize>>;
    async fn read_resource(&self, name: &str) -> Result<Vec<u8>>;
    fn set_timer(&mut self, id: WIPICWord, due: Instant, callback: WIPICMethodBody);
    fn unset_timer(&mut self, id: WIPICWord);

    /// Which of the two pixels a title's `PixelopIdx` operation is handed
    /// first. The two handsets disagree, and a title is written against the one
    /// it shipped on.
    ///
    /// LGT hands it the destination: 드래곤하트2's operation @0x2618 is
    /// `if (b == transparentColour) return a` and then lerps `a` towards `b` -
    /// keeping `a` when `b` has nothing to say is only a blit if `a` is what is
    /// already on screen. The firmware agrees: `dgraphics_set_pixel_operation`
    /// plants a span at ctx+0x58 called as `f(dst*, src*, count, param)`.
    ///
    /// KTF hands it the source: 마스터오브소드4's @0x10c89c is `if (a != white)
    /// return a; else return textColour`, which recolours the one white its
    /// glyph strips carry and never looks at the second pixel at all. Handed
    /// the destination it answers the box the glyph lands on, and the title
    /// draws no text anywhere.
    fn pixel_op_takes_source_first(&self) -> bool {
        false
    }

    /// The order this handset keeps a graphics context's two colour words in.
    ///
    /// See `ContextLayout`. LGT's is the one the API is usually written down
    /// with, so it is the default and KTF is what overrides it.
    fn graphics_context_layout(&self) -> ContextLayout {
        ContextLayout::ForegroundFirst
    }
}

pub struct WIPICResult {
    pub results: Vec<WIPICWord>,
}

impl ParamConverter<WIPICWord> for WIPICWord {
    fn convert(_: &mut dyn WIPICContext, raw: WIPICWord) -> WIPICWord {
        raw
    }
}

impl ParamConverter<WIPICIndirectPtr> for WIPICIndirectPtr {
    fn convert(_: &mut dyn WIPICContext, raw: WIPICWord) -> WIPICIndirectPtr {
        WIPICIndirectPtr(raw)
    }
}

impl ParamConverter<i32> for i32 {
    fn convert(_: &mut dyn WIPICContext, raw: WIPICWord) -> i32 {
        raw as _
    }
}

impl ResultConverter<u64> for u64 {
    fn convert(_: &mut dyn WIPICContext, result: u64) -> WIPICResult {
        WIPICResult {
            results: vec![result as u32, (result >> 32) as u32],
        }
    }
}

impl ResultConverter<WIPICWord> for WIPICWord {
    fn convert(_: &mut dyn WIPICContext, result: WIPICWord) -> WIPICResult {
        WIPICResult { results: vec![result] }
    }
}

impl ResultConverter<WIPICIndirectPtr> for WIPICIndirectPtr {
    fn convert(_: &mut dyn WIPICContext, result: WIPICIndirectPtr) -> WIPICResult {
        WIPICResult { results: vec![result.0] }
    }
}

impl ResultConverter<i32> for i32 {
    fn convert(_: &mut dyn WIPICContext, result: i32) -> WIPICResult {
        WIPICResult { results: vec![result as _] }
    }
}

impl ResultConverter<()> for () {
    fn convert(_: &mut dyn WIPICContext, _: ()) -> WIPICResult {
        WIPICResult { results: Vec::new() }
    }
}

#[cfg(test)]
pub mod test {
    use alloc::{boxed::Box, format, string::String, vec::Vec};

    use wipi_types::wipic::{WIPICIndirectPtr, WIPICWord};

    use wie_backend::{Instant, System};
    use wie_util::{ByteRead, ByteWrite, Result, WieError};

    use super::{WIPICContext, WIPICMethodBody};
    use crate::api::{
        filesystem::{SharedFilesystemState, new_state as new_filesystem_state},
        graphics::ContextLayout,
        im::{SharedImState, new_state as new_im_state},
        kernel::{SharedKernelState, new_state as new_kernel_state},
        net::{SharedNetworkState, new_state as new_network_state},
        serial::{SharedSerialState, new_state as new_serial_state},
        shared_buf::{SharedSharedBufState, new_state as new_shared_buf_state},
    };

    const TEST_MEMORY_SIZE: usize = 0x20000;
    const TEST_ALLOC_START: usize = 0x10000;
    const TEST_GLOBAL_DATA_BASE: u32 = 0x7fff_0000;
    const TEST_GLOBAL_DATA_SIZE: usize = 0x4000;

    pub struct TestContext {
        memory: [u8; TEST_MEMORY_SIZE],
        global_data: [u8; TEST_GLOBAL_DATA_SIZE],
        last_alloc: usize,
        raw_allocations: Vec<(WIPICWord, WIPICWord)>,
        system: Option<System>,
        resources: Vec<(String, Vec<u8>)>,
        network_state: SharedNetworkState,
        serial_state: SharedSerialState,
        filesystem_state: SharedFilesystemState,
        shared_buf_state: SharedSharedBufState,
        im_state: SharedImState,
        kernel_state: SharedKernelState,
        /// Bodies handed to `spawn`, kept rather than run: a test that drives an
        /// API which defers work can then say the deferral happened without an
        /// executor to run it on.
        spawned: Vec<WIPICMethodBody>,
        /// Stands in for code of the title's own, so a test can drive an API
        /// that calls back into it - a pixel operation, a database comparator -
        /// without an ARM core to run it on. The address is passed through, so
        /// one function can answer for several.
        guest_function: Option<fn(WIPICWord, &[WIPICWord]) -> WIPICWord>,
        /// Which handset the test is standing in for, where that decides what
        /// an API does - so far only the order a pixel operation is asked in.
        pixel_op_takes_source_first: bool,
        /// The order this handset keeps a context's two colour words in.
        graphics_context_layout: ContextLayout,
    }

    impl TestContext {
        /// Installs code that stands in for the title's own.
        pub fn set_guest_function(&mut self, function: fn(WIPICWord, &[WIPICWord]) -> WIPICWord) {
            self.guest_function = Some(function);
        }

        /// Stands in for a KTF handset, which asks a pixel operation about the
        /// source first.
        pub fn set_pixel_op_takes_source_first(&mut self, source_first: bool) {
            self.pixel_op_takes_source_first = source_first;
        }

        /// Stands in for a handset that keeps a context's colour words in the
        /// given order - see `ContextLayout`.
        pub fn set_graphics_context_layout(&mut self, layout: ContextLayout) {
            self.graphics_context_layout = layout;
        }

        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            Self {
                memory: [0; TEST_MEMORY_SIZE],
                global_data: [0; TEST_GLOBAL_DATA_SIZE],
                last_alloc: TEST_ALLOC_START,
                raw_allocations: Vec::new(),
                system: None,
                resources: Vec::new(),
                network_state: new_network_state(),
                serial_state: new_serial_state(),
                filesystem_state: new_filesystem_state(),
                shared_buf_state: new_shared_buf_state(),
                im_state: new_im_state(),
                kernel_state: new_kernel_state(),
                spawned: Vec::new(),
                guest_function: None,
                pixel_op_takes_source_first: false,
                graphics_context_layout: ContextLayout::ForegroundFirst,
            }
        }

        pub fn with_system(system: System) -> Self {
            Self {
                memory: [0; TEST_MEMORY_SIZE],
                global_data: [0; TEST_GLOBAL_DATA_SIZE],
                last_alloc: TEST_ALLOC_START,
                raw_allocations: Vec::new(),
                system: Some(system),
                resources: Vec::new(),
                network_state: new_network_state(),
                serial_state: new_serial_state(),
                filesystem_state: new_filesystem_state(),
                shared_buf_state: new_shared_buf_state(),
                im_state: new_im_state(),
                kernel_state: new_kernel_state(),
                spawned: Vec::new(),
                guest_function: None,
                pixel_op_takes_source_first: false,
                graphics_context_layout: ContextLayout::ForegroundFirst,
            }
        }

        /// How many bodies have been handed to `spawn`.
        pub fn spawned(&self) -> usize {
            self.spawned.len()
        }

        /// The allocations this context has handed out and not taken back.
        pub fn live_allocations(&self) -> Vec<(WIPICWord, WIPICWord)> {
            self.raw_allocations.clone()
        }

        /// Takes the spawned bodies, for a test that wants to run the deferred
        /// work itself.
        pub fn take_spawned(&mut self) -> Vec<WIPICMethodBody> {
            core::mem::take(&mut self.spawned)
        }

        pub fn with_resource(mut self, name: &str, data: &[u8]) -> Self {
            self.resources.push((String::from(name), data.to_vec()));
            self
        }
    }

    #[async_trait::async_trait]
    impl WIPICContext for TestContext {
        fn alloc_raw(&mut self, size: WIPICWord) -> Result<WIPICWord> {
            let address = self.last_alloc;
            self.last_alloc += size as usize;
            self.raw_allocations.push((address as WIPICWord, size));

            Ok(address as WIPICWord)
        }

        fn alloc(&mut self, size: WIPICWord) -> Result<WIPICIndirectPtr> {
            Ok(WIPICIndirectPtr(Self::alloc_raw(self, size)?))
        }

        fn free(&mut self, memory: WIPICIndirectPtr) -> Result<()> {
            // Forget it the way `free_raw` does, so a test can ask what a call
            // gave back: `alloc` and `alloc_raw` hand out the same addresses
            // here, and a leak is a row left in `raw_allocations`.
            if let Some(index) = self.raw_allocations.iter().position(|&(candidate, _)| candidate == memory.0) {
                self.raw_allocations.remove(index);
            }

            Ok(())
        }

        fn free_raw(&mut self, address: WIPICWord, _size: WIPICWord) -> Result<()> {
            if let Some(index) = self.raw_allocations.iter().position(|&(candidate, _)| candidate == address) {
                self.raw_allocations.remove(index);
            }
            Ok(())
        }

        fn free_raw_unsized(&mut self, address: WIPICWord) -> Result<()> {
            if let Some(index) = self.raw_allocations.iter().position(|&(candidate, _)| candidate == address) {
                self.raw_allocations.remove(index);
            }
            Ok(())
        }

        fn raw_alloc_size(&self, address: WIPICWord) -> Result<WIPICWord> {
            self.raw_allocations
                .iter()
                .find(|&&(candidate, _)| candidate == address)
                .map(|&(_, size)| size)
                .ok_or_else(|| WieError::FatalError(format!("Address {address:#x} is not a tracked raw allocation")))
        }

        fn data_ptr(&self, memory: WIPICIndirectPtr) -> Result<WIPICWord> {
            Ok(memory.0)
        }

        async fn call_function(&mut self, address: WIPICWord, args: &[WIPICWord]) -> Result<WIPICWord> {
            match self.guest_function {
                Some(function) => Ok(function(address, args)),
                None => todo!(),
            }
        }

        fn pixel_op_takes_source_first(&self) -> bool {
            self.pixel_op_takes_source_first
        }

        fn graphics_context_layout(&self) -> ContextLayout {
            self.graphics_context_layout
        }

        fn system(&mut self) -> &mut System {
            self.system.as_mut().unwrap()
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

        fn kernel_state(&self) -> SharedKernelState {
            self.kernel_state.clone()
        }

        fn spawn(&mut self, callback: WIPICMethodBody) -> Result<()> {
            // Kept rather than run: there is no executor here, and a test that
            // needs the deferred work to happen calls it itself.
            self.spawned.push(callback);

            Ok(())
        }

        async fn get_resource_size(&self, name: &str) -> Result<Option<usize>> {
            Ok(self.resources.iter().find(|(x, _)| x == name).map(|(_, data)| data.len()))
        }

        async fn read_resource(&self, name: &str) -> Result<Vec<u8>> {
            self.resources
                .iter()
                .find(|(x, _)| x == name)
                .map(|(_, data)| data.clone())
                .ok_or_else(|| WieError::FatalError(format!("Missing test resource: {name}")))
        }

        fn set_timer(&mut self, id: WIPICWord, due: Instant, _callback: WIPICMethodBody) {
            if let Some(system) = self.system.as_mut() {
                system.event_queue().push_timer(id, due, || async { Ok(()) });
            }
        }

        fn unset_timer(&mut self, id: WIPICWord) {
            if let Some(system) = self.system.as_mut() {
                system.event_queue().cancel_timer(id);
            }
        }
    }

    impl ByteWrite for TestContext {
        fn write_bytes(&mut self, address: u32, data: &[u8]) -> wie_util::Result<()> {
            if (TEST_GLOBAL_DATA_BASE..TEST_GLOBAL_DATA_BASE + TEST_GLOBAL_DATA_SIZE as u32).contains(&address) {
                let start = (address - TEST_GLOBAL_DATA_BASE) as usize;
                let end = start + data.len();
                if end > TEST_GLOBAL_DATA_SIZE {
                    return Err(WieError::InvalidMemoryAccess(address));
                }
                self.global_data[start..end].copy_from_slice(data);
                return Ok(());
            }

            let start = address as usize;
            let end = start + data.len();
            if end > TEST_MEMORY_SIZE {
                return Err(WieError::InvalidMemoryAccess(address));
            }
            self.memory[start..end].copy_from_slice(data);

            Ok(())
        }
    }

    impl ByteRead for TestContext {
        fn read_bytes(&self, address: u32, result: &mut [u8]) -> wie_util::Result<usize> {
            if (TEST_GLOBAL_DATA_BASE..TEST_GLOBAL_DATA_BASE + TEST_GLOBAL_DATA_SIZE as u32).contains(&address) {
                let start = (address - TEST_GLOBAL_DATA_BASE) as usize;
                let end = start + result.len();
                if end > TEST_GLOBAL_DATA_SIZE {
                    return Err(WieError::InvalidMemoryAccess(address));
                }
                result.copy_from_slice(&self.global_data[start..end]);
                return Ok(result.len());
            }

            let start = address as usize;
            let end = start + result.len();
            if end > TEST_MEMORY_SIZE {
                return Err(WieError::InvalidMemoryAccess(address));
            }
            result.copy_from_slice(&self.memory[start..end]);

            Ok(result.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::{ResultConverter, test::TestContext};

    #[test]
    fn convert_u64_splits_low_and_high_words() {
        let mut context = TestContext::new();

        let result = <u64 as ResultConverter<u64>>::convert(&mut context, 0x1122_3344_5566_7788);

        assert_eq!(result.results, vec![0x5566_7788, 0x1122_3344]);
    }
}
