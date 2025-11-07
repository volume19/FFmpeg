//! Frame pool for efficient memory reuse
//!
//! Reduces allocation overhead by reusing frame buffers.
//! Phase 2: Simple pool with configurable capacity
//! Phase 3: NUMA-aware pools for multi-socket systems

use crate::{Frame, PixelFormat};
use std::sync::{Arc, Mutex};

/// Thread-safe frame buffer pool
///
/// Reuses allocated frame buffers to avoid repeated allocations.
/// Particularly important for high-throughput video processing.
///
/// # Example
/// ```
/// use av_core::{FramePool, PixelFormat};
///
/// let pool = FramePool::new(PixelFormat::Yuv420p, 1920, 1080, 8);
///
/// // Allocate frame from pool
/// let frame = pool.get();
///
/// // Use frame...
///
/// // Return to pool (automatic on drop if using PooledFrame)
/// drop(frame);
/// ```
pub struct FramePool {
    pixel_format: PixelFormat,
    width: usize,
    height: usize,
    pool: Arc<Mutex<Vec<Frame>>>,
    max_size: usize,
}

impl FramePool {
    /// Create a new frame pool
    ///
    /// # Arguments
    /// * `pixel_format` - Pixel format for all frames
    /// * `width` - Frame width in pixels
    /// * `height` - Frame height in pixels
    /// * `max_size` - Maximum number of frames to cache
    pub fn new(pixel_format: PixelFormat, width: usize, height: usize, max_size: usize) -> Self {
        Self {
            pixel_format,
            width,
            height,
            pool: Arc::new(Mutex::new(Vec::with_capacity(max_size))),
            max_size,
        }
    }

    /// Get a frame from the pool (or allocate new if pool is empty)
    pub fn get(&self) -> Frame {
        let mut pool = self.pool.lock().unwrap();

        if let Some(mut frame) = pool.pop() {
            // Reuse existing frame, just reset metadata
            frame.pts = None;
            frame.duration = None;
            frame
        } else {
            // Allocate new frame
            Frame::new_video(self.width, self.height, self.pixel_format)
        }
    }

    /// Return a frame to the pool
    ///
    /// Only returns if pool is not full. Otherwise, frame is dropped.
    pub fn put(&self, frame: Frame) {
        // Validate frame matches pool configuration
        if frame.width != self.width
            || frame.height != self.height
            || frame.pixel_format != Some(self.pixel_format)
        {
            // Wrong size/format, just drop it
            return;
        }

        let mut pool = self.pool.lock().unwrap();
        if pool.len() < self.max_size {
            pool.push(frame);
        }
        // else: pool full, drop the frame
    }

    /// Get current pool size
    pub fn len(&self) -> usize {
        self.pool.lock().unwrap().len()
    }

    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.pool.lock().unwrap().is_empty()
    }

    /// Clear all frames from pool
    pub fn clear(&self) {
        self.pool.lock().unwrap().clear();
    }

    /// Get pool capacity
    pub fn capacity(&self) -> usize {
        self.max_size
    }
}

impl Clone for FramePool {
    fn clone(&self) -> Self {
        Self {
            pixel_format: self.pixel_format,
            width: self.width,
            height: self.height,
            pool: Arc::clone(&self.pool),
            max_size: self.max_size,
        }
    }
}

/// RAII wrapper for pooled frames
///
/// Automatically returns frame to pool on drop.
pub struct PooledFrame {
    frame: Option<Frame>,
    pool: FramePool,
}

impl PooledFrame {
    /// Create a new pooled frame
    pub fn new(pool: FramePool) -> Self {
        let frame = pool.get();
        Self {
            frame: Some(frame),
            pool,
        }
    }

    /// Get reference to underlying frame
    pub fn frame(&self) -> &Frame {
        self.frame.as_ref().unwrap()
    }

    /// Get mutable reference to underlying frame
    pub fn frame_mut(&mut self) -> &mut Frame {
        self.frame.as_mut().unwrap()
    }

    /// Take ownership of frame (won't return to pool)
    pub fn take(mut self) -> Frame {
        self.frame.take().unwrap()
    }
}

impl Drop for PooledFrame {
    fn drop(&mut self) {
        if let Some(frame) = self.frame.take() {
            self.pool.put(frame);
        }
    }
}

impl std::ops::Deref for PooledFrame {
    type Target = Frame;

    fn deref(&self) -> &Self::Target {
        self.frame()
    }
}

impl std::ops::DerefMut for PooledFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.frame_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_basic() {
        let pool = FramePool::new(PixelFormat::Yuv420p, 1920, 1080, 4);

        assert_eq!(pool.len(), 0);
        assert!(pool.is_empty());

        let frame1 = pool.get();
        assert_eq!(frame1.width, 1920);
        assert_eq!(frame1.height, 1080);

        pool.put(frame1);
        assert_eq!(pool.len(), 1);

        let frame2 = pool.get();
        assert_eq!(pool.len(), 0);

        pool.put(frame2);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_pool_max_size() {
        let pool = FramePool::new(PixelFormat::Yuv420p, 640, 480, 2);

        let f1 = pool.get();
        let f2 = pool.get();
        let f3 = pool.get();

        pool.put(f1);
        pool.put(f2);
        pool.put(f3); // Should be dropped (pool full)

        assert_eq!(pool.len(), 2); // Max 2 frames
    }

    #[test]
    fn test_pooled_frame_raii() {
        let pool = FramePool::new(PixelFormat::Yuv420p, 1280, 720, 4);

        {
            let _pooled = PooledFrame::new(pool.clone());
            assert_eq!(pool.len(), 0); // Frame is checked out
        }

        // Frame should be returned to pool on drop
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_pooled_frame_take() {
        let pool = FramePool::new(PixelFormat::Yuv420p, 800, 600, 4);

        let pooled = PooledFrame::new(pool.clone());
        let _frame = pooled.take(); // Takes ownership, won't return to pool

        assert_eq!(pool.len(), 0); // Not returned
    }

    #[test]
    fn test_pool_clone() {
        let pool1 = FramePool::new(PixelFormat::Yuv420p, 1920, 1080, 4);
        let pool2 = pool1.clone();

        let frame = pool1.get();
        pool2.put(frame);

        // Both pools share the same underlying storage
        assert_eq!(pool1.len(), 1);
        assert_eq!(pool2.len(), 1);
    }
}
