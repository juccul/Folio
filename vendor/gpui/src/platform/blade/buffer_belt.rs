use super::upload_pool::UploadPool;
use blade_graphics as gpu;
pub(super) use blade_util::BufferBeltDescriptor;

pub(super) struct BufferBelt {
    pool: UploadPool<gpu::Buffer, gpu::SyncPoint>,
    memory: gpu::Memory,
    alignment: u64,
}

impl BufferBelt {
    pub fn new(desc: BufferBeltDescriptor) -> Self {
        Self {
            pool: UploadPool::new(desc.min_chunk_size, desc.alignment),
            memory: desc.memory,
            alignment: desc.alignment,
        }
    }

    fn alloc(&mut self, size: u64, gpu: &gpu::Context) -> gpu::BufferPiece {
        let memory = self.memory;
        let (buffer, offset) = self.pool.allocate(
            size,
            |capacity| {
                gpu.create_buffer(gpu::BufferDesc {
                    name: "frame upload",
                    size: capacity,
                    memory,
                })
            },
            |sync| gpu.wait_for(sync, 0),
            |buffer| gpu.destroy_buffer(buffer),
        );
        buffer.at(offset)
    }

    pub fn alloc_bytes(&mut self, bytes: &[u8], gpu: &gpu::Context) -> gpu::BufferPiece {
        let piece = self.alloc(bytes.len() as u64, gpu);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), piece.data(), bytes.len());
        }
        piece
    }

    /// T must contain only initialized plain data, with no references or pointers.
    pub unsafe fn alloc_typed<T>(&mut self, data: &[T], gpu: &gpu::Context) -> gpu::BufferPiece {
        debug_assert_eq!(self.alignment % std::mem::align_of::<T>() as u64, 0);
        let byte_len = std::mem::size_of_val(data);
        let piece = self.alloc(byte_len as u64, gpu);
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().cast::<u8>(), piece.data(), byte_len);
        }
        piece
    }

    pub fn flush(&mut self, sync: &gpu::SyncPoint, gpu: &gpu::Context) {
        self.pool.finish(
            sync,
            |sync| gpu.wait_for(sync, 0),
            |buffer| gpu.destroy_buffer(buffer),
        );
    }

    pub fn destroy(&mut self, gpu: &gpu::Context) {
        self.pool.destroy(|buffer| gpu.destroy_buffer(buffer));
    }
}
