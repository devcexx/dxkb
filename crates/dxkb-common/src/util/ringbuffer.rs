use core::mem::{self, MaybeUninit};

/// A fixed-capacity ring buffer backed by `[MaybeUninit<T>; N]`.
///
/// The buffer tracks its contents via two monotonically increasing (wrapping)
/// pointers `readptr` and `writeptr`. The actual index into the backing array
/// is obtained via `ptr % N`. The logical length is `writeptr.wrapping_sub(readptr)`.
///
/// `N` must be greater than zero; this is enforced at the type level.
pub struct RingBuffer<T, const N: usize>
{
    buf: [MaybeUninit<T>; N],
    readptr: usize,
    writeptr: usize,
}

impl<T, const N: usize> RingBuffer<T, N>
{
    const fn assert_valid_length() {
        assert!(N > 0, "Ringuffer capacity must be greater then zero!")
    }

    /// Creates a new, empty ring buffer. The backing storage is not
    /// initialized, making construction essentially free.
    #[inline]
    pub const fn new() -> Self {
        const { Self::assert_valid_length() }

        Self {
            buf: [const { MaybeUninit::<T>::uninit() }; N],
            readptr: 0,
            writeptr: 0,
        }
    }

    /// Returns the number of elements currently stored in the buffer.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.writeptr - self.readptr
    }

    /// Returns the number of free slots in te buffer.
    #[inline(always)]
    pub const fn free(&self) -> usize {
        N - self.len()
    }

    /// Returns `true` if the buffer contains no elements.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.readptr == self.writeptr
    }

    /// Returns `true` if the buffer is at full capacity.
    #[inline(always)]
    pub const fn is_full(&self) -> bool {
        self.len() == N
    }

    /// Returns the total capacity of the buffer.
    #[inline(always)]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Maps a logical pointer value to an index in the backing array.
    #[inline(always)]
    const fn index_of(ptr: usize) -> usize {
        ptr % N
    }

    #[inline(always)]
    fn get_slot_at_ptr(&self, ptr: usize) -> &MaybeUninit<T> {
        unsafe {
            // SAFETY: `index_of` always returns a value in `[0, N)`.
            self.buf.get_unchecked(Self::index_of(ptr))
        }

    }
    #[inline(always)]
    fn get_slot_at_ptr_mut(&mut self, ptr: usize) -> &mut MaybeUninit<T> {
        unsafe {
            // SAFETY: `index_of` always returns a value in `[0, N)`.
            self.buf.get_unchecked_mut(Self::index_of(ptr))
        }
    }


    /// Pushes an element onto the back of the buffer.
    ///
    /// If the buffer is full the oldest (front) element is overwritten and
    /// dropped.
    #[inline]
    pub fn push(&mut self, value: T) {
        if self.is_full() {
            self.drop_first(1);
        }
        *self.get_slot_at_ptr_mut(self.writeptr) = MaybeUninit::new(value);
        self.writeptr += 1;
    }

    /// Removes and returns the first (oldest) element, or `None` if empty.
    #[inline]
    pub fn poll_first(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let res = mem::replace(self.get_slot_at_ptr_mut(self.readptr), MaybeUninit::uninit());
        self.readptr += 1;
        Some(unsafe {
            // SAFETY: The buffer is not empty.
            res.assume_init()
        })
    }

    /// Returns a reference to the first (oldest) element, or `None` if empty.
    #[inline]
    pub fn peek_first(&self) -> Option<&T> {
        if self.is_empty() {
            return None;
        }

        Some(unsafe {
            // SAFETY: The buffer is non-empty, so the slot at `readptr` is
            // initialised.
            self.get_slot_at_ptr(self.readptr).assume_init_ref()
        })
    }

    /// Returns a mutable reference to the first (oldest) element, or `None` if empty.
    #[inline]
    pub fn peek_first_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            return None;
        }

        Some(unsafe {
            // SAFETY: The buffer is non-empty, so the slot at `readptr` is
            // initialised.
            self.get_slot_at_ptr_mut(self.readptr).assume_init_mut()
        })
    }

    /// Removes and returns the last (newest) element, or `None` if empty.
    #[inline]
    pub fn poll_last(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        self.writeptr -= 1;
        Some(unsafe {
            // SAFETY: Before decrementing, writeptr pointed one past the last
            // initialised slot, so after decrementing it points to the last
            // initialised element.
            mem::replace(self.get_slot_at_ptr_mut(self.writeptr), MaybeUninit::uninit()).assume_init()
        })
    }

    /// Returns a reference to the last (newest) element, or `None` if empty.
    #[inline]
    pub fn peek_last(&self) -> Option<&T> {
        if self.is_empty() {
            return None;
        }

        Some(unsafe {
            // SAFETY: The buffer is non-empty so there is at least one
            // initialised slot. `writeptr - 1` is the index of the newest
            // element.
            self.get_slot_at_ptr(self.writeptr - 1).assume_init_ref()
        })
    }


    /// Returns a reference to the last (newest) element, or `None` if empty.
    #[inline]
    pub fn peek_last_mut(&mut self) -> Option<&mut T> {
        if self.is_empty() {
            return None;
        }

        Some(unsafe {
            // SAFETY: The buffer is non-empty so there is at least one
            // initialised slot. `writeptr - 1` is the index of the newest
            // element.
            self.get_slot_at_ptr_mut(self.writeptr - 1).assume_init_mut()
        })
    }


    /// Copies up to `buf.len()` elements from the front of the ring buffer
    /// into `buf` **without** removing them.
    ///
    /// Returns the number of elements actually copied (may be less than
    /// `buf.len()` when fewer elements are available).
    #[inline]
    pub fn read(&self, buf: &mut [T]) -> usize
    where
        T: Copy,
    {
        let count = usize::min(buf.len(), self.len());
        let start_idx = Self::index_of(self.readptr);

        if start_idx + count <= N {
            unsafe {
                core::ptr::copy_nonoverlapping(self.buf.as_ptr().add(start_idx).cast(), buf.as_mut_ptr(), count);
            }
        } else {
            unsafe {
                // Read until the end of the buffer and then do another read of the remaining bytes from the beginning of the buffer.
                let first_read_cnt = N - start_idx;
                core::ptr::copy_nonoverlapping(self.buf.as_ptr().add(start_idx).cast(), buf.as_mut_ptr(), first_read_cnt);
                core::ptr::copy_nonoverlapping(self.buf.as_ptr().cast(), buf.as_mut_ptr().add(first_read_cnt), count - first_read_cnt);
            }
        }

        count
    }

    /// Copies elements from `buf` into the ring buffer, overwriting the existing ones. Panics if `buf.len() > N`.
    pub fn write(&mut self, buf: &[T])
    where T: Copy {
        if buf.len() > N {
            panic!("Buffer is too big for the max ring buffer size ({} > {})", buf.len(), N);
        }

        if buf.len() > self.free() {
            // If the new data doesn't fit into the free space, we need to drop some of the oldest elements to make room for it.
            self.drop_first(buf.len() - self.free());
        }

        let start_idx = Self::index_of(self.writeptr);
        if start_idx + buf.len() <= N {
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), self.buf.as_mut_ptr().add(start_idx).cast(), buf.len());
            }
        } else {
            let first_write_cnt = N - start_idx;
            let second_write_cnt = buf.len() - first_write_cnt;

            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), self.buf.as_mut_ptr().add(start_idx).cast(), first_write_cnt);
                core::ptr::copy_nonoverlapping(buf.as_ptr().add(first_write_cnt), self.buf.as_mut_ptr().cast(), second_write_cnt);
            }
        }
        self.writeptr += buf.len()
    }

    /// Drops up to `n` elements from the front (oldest end) of the buffer.
    ///
    /// Returns the number of elements actually dropped.
    #[inline]
    pub fn drop_first(&mut self, n: usize) -> usize {
        let count = usize::min(self.len(), n);

        if mem::needs_drop::<T>() {
            for i in 0..count {
                unsafe {
                    // SAFETY: `i < count <= self.len()` so the slot is
                    // initialised.
                    let _ = mem::replace(self.get_slot_at_ptr_mut(self.readptr + i), MaybeUninit::uninit()).assume_init();
                }
            }
        }

        self.readptr += count;
        count
    }

    /// Drops up to `n` elements from the back (newest end) of the buffer.
    ///
    /// Returns the number of elements actually dropped.
    #[inline]
    pub fn drop_last(&mut self, n: usize) -> usize {
        let count = usize::min(self.len(), n);

        if mem::needs_drop::<T>() {
            for i in 0..count {
                unsafe {
                    // SAFETY: Walking backwards from writeptr, each slot within
                    // `count` is initialised.
                    let _ = mem::replace(self.get_slot_at_ptr_mut(self.writeptr - i - 1), MaybeUninit::uninit()).assume_init();
                }
            }
        }

        self.writeptr -= count;
        count
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::RingBuffer;
    use std::cell::Cell;

    #[derive(Debug)]
    struct DropCounter<'a> {
        id: u32,
        counter: &'a Cell<u32>,
    }

    impl<'a> DropCounter<'a> {
        fn new(id: u32, counter: &'a Cell<u32>) -> Self {
            Self { id, counter }
        }
    }

    impl Drop for DropCounter<'_> {
        fn drop(&mut self) {
            self.counter.set(self.counter.get() + 1);
        }
    }

    #[test]
    fn test_new_with_capacity_4() {
        let rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.len(), 0);
        assert!(rb.is_empty());
        assert!(!rb.is_full());
        assert_eq!(rb.capacity(), 4);
    }

    #[test]
    fn test_new_with_capacity_1() {
        let rb = RingBuffer::<i32, 1>::new();
        assert_eq!(rb.len(), 0);
        assert!(rb.is_empty());
        assert!(!rb.is_full());
        assert_eq!(rb.capacity(), 1);
    }

    #[test]
    fn test_metadata_empty() {
        let rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.len(), 0);
        assert!(rb.is_empty());
        assert!(!rb.is_full());
    }

    #[test]
    fn test_metadata_one_element() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        assert_eq!(rb.len(), 1);
        assert!(!rb.is_empty());
        assert!(!rb.is_full());
    }

    #[test]
    fn test_metadata_full() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 4);
        assert!(!rb.is_empty());
        assert!(rb.is_full());
    }

    #[test]
    fn test_metadata_after_overwrite() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=5 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 4);
        assert!(rb.is_full());
    }

    #[test]
    fn test_metadata_push_then_poll() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.poll_first();
        assert_eq!(rb.len(), 0);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_capacity_always_returns_n() {
        let mut rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.capacity(), 4);
        rb.push(1);
        assert_eq!(rb.capacity(), 4);
        for i in 2..=4 {
            rb.push(i);
        }
        assert_eq!(rb.capacity(), 4);
        rb.push(5); // overwrite
        assert_eq!(rb.capacity(), 4);
        rb.poll_first();
        assert_eq!(rb.capacity(), 4);
    }

    #[test]
    fn test_push_to_empty() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(10);
        assert_eq!(rb.len(), 1);
        assert_eq!(rb.peek_first(), Some(&10));
        assert_eq!(rb.peek_last(), Some(&10));
    }

    #[test]
    fn test_push_until_full() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 4);
        assert!(rb.is_full());
        assert_eq!(rb.peek_first(), Some(&1));
        assert_eq!(rb.peek_last(), Some(&4));
    }

    #[test]
    fn test_push_past_capacity_evicts_oldest() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=5 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 4);
        assert_eq!(rb.peek_first(), Some(&2));
        assert_eq!(rb.peek_last(), Some(&5));
    }

    #[test]
    fn test_push_many_past_capacity() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=12 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 4);
        // Only last 4 survive: 9, 10, 11, 12
        for expected in 9..=12 {
            assert_eq!(rb.poll_first(), Some(expected));
        }
        assert!(rb.is_empty());
    }

    #[test]
    fn test_push_into_capacity_1() {
        let mut rb = RingBuffer::<i32, 1>::new();
        rb.push(100);
        assert_eq!(rb.len(), 1);
        assert_eq!(rb.peek_first(), Some(&100));

        rb.push(200);
        assert_eq!(rb.len(), 1);
        assert_eq!(rb.peek_first(), Some(&200));
    }

    #[test]
    fn test_poll_first_empty() {
        let mut rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.poll_first(), None);
    }

    #[test]
    fn test_poll_first_single() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(42);
        assert_eq!(rb.poll_first(), Some(42));
        assert!(rb.is_empty());
    }

    #[test]
    fn test_poll_first_fifo_order() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.poll_first(), Some(1));
        assert_eq!(rb.poll_first(), Some(2));
        assert_eq!(rb.poll_first(), Some(3));
    }

    #[test]
    fn test_poll_first_after_overwrite() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=5 {
            rb.push(i);
        }
        // 1 was evicted, remaining: 2, 3, 4, 5
        assert_eq!(rb.poll_first(), Some(2));
        assert_eq!(rb.poll_first(), Some(3));
        assert_eq!(rb.poll_first(), Some(4));
        assert_eq!(rb.poll_first(), Some(5));
    }

    #[test]
    fn test_interleaved_push_poll() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.poll_first(), Some(1));
        rb.push(3);
        assert_eq!(rb.poll_first(), Some(2));
        assert_eq!(rb.poll_first(), Some(3));
        assert_eq!(rb.poll_first(), None);
    }

    #[test]
    fn test_poll_first_capacity_1() {
        let mut rb = RingBuffer::<i32, 1>::new();
        rb.push(99);
        assert_eq!(rb.poll_first(), Some(99));
        assert!(rb.is_empty());
    }

    #[test]
    fn test_poll_last_empty() {
        let mut rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.poll_last(), None);
    }

    #[test]
    fn test_poll_last_single() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(42);
        assert_eq!(rb.poll_last(), Some(42));
        assert!(rb.is_empty());
    }

    #[test]
    fn test_poll_last_returns_newest() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.poll_last(), Some(3));
        assert_eq!(rb.len(), 2);
    }

    #[test]
    fn test_poll_last_lifo_drain() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.poll_last(), Some(3));
        assert_eq!(rb.poll_last(), Some(2));
        assert_eq!(rb.poll_last(), Some(1));
        assert!(rb.is_empty());
    }

    #[test]
    fn test_poll_last_after_overwrite() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=5 {
            rb.push(i);
        }
        assert_eq!(rb.poll_last(), Some(5));
    }

    #[test]
    fn test_peek_first_empty() {
        let rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.peek_first(), None);
    }

    #[test]
    fn test_peek_first_mut_empty() {
        let mut rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.peek_first_mut(), None);
    }

    #[test]
    fn test_peek_first_returns_oldest() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.peek_first(), Some(&1));
    }

    #[test]
    fn test_peek_first_does_not_consume() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.peek_first(), Some(&1));
        assert_eq!(rb.peek_first(), Some(&1));
        assert_eq!(rb.len(), 2);
    }

    #[test]
    fn test_peek_first_after_overwrite() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=5 {
            rb.push(i);
        }
        assert_eq!(rb.peek_first(), Some(&2));
    }

    #[test]
    fn test_peek_first_mut_modify() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        *rb.peek_first_mut().unwrap() = 99;
        assert_eq!(rb.peek_first(), Some(&99));
        assert_eq!(rb.poll_first(), Some(99));
    }

    #[test]
    fn test_peek_last_empty() {
        let rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.peek_last(), None);
    }

    #[test]
    fn test_peek_last_mut_empty() {
        let mut rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.peek_last_mut(), None);
    }

    #[test]
    fn test_peek_last_returns_newest() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.peek_last(), Some(&3));
    }

    #[test]
    fn test_peek_last_does_not_consume() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.peek_last(), Some(&2));
        assert_eq!(rb.peek_last(), Some(&2));
        assert_eq!(rb.len(), 2);
    }

    #[test]
    fn test_peek_last_after_overwrite() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=5 {
            rb.push(i);
        }
        assert_eq!(rb.peek_last(), Some(&5));
    }

    #[test]
    fn test_peek_last_mut_modify() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        *rb.peek_last_mut().unwrap() = 99;
        assert_eq!(rb.peek_last(), Some(&99));
        assert_eq!(rb.poll_last(), Some(99));
    }

    #[test]
    fn test_read_from_empty() {
        let rb = RingBuffer::<i32, 4>::new();
        let mut buf = [0i32; 4];
        let n = rb.read(&mut buf);
        assert_eq!(n, 0);
        assert_eq!(buf, [0; 4]); // unchanged
    }

    #[test]
    fn test_read_fewer_than_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        let mut buf = [0i32; 2];
        let n = rb.read(&mut buf);
        assert_eq!(n, 2);
        assert_eq!(buf, [1, 2]);
        // Non-destructive: length unchanged
        assert_eq!(rb.len(), 4);
    }

    #[test]
    fn test_read_exactly_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        let mut buf = [0i32; 4];
        let n = rb.read(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    fn test_read_more_than_available() {
        let mut rb = RingBuffer::<i32, 8>::new();
        rb.push(1);
        rb.push(2);
        let mut buf = [0i32; 5];
        let n = rb.read(&mut buf);
        assert_eq!(n, 2);
        assert_eq!(buf[0], 1);
        assert_eq!(buf[1], 2);
    }

    #[test]
    fn test_read_across_wrap_boundary() {
        let mut rb = RingBuffer::<i32, 4>::new();
        // Fill and partially drain to shift the internal pointers
        for i in 1..=4 {
            rb.push(i);
        }
        rb.poll_first(); // removes 1, readptr=1
        rb.poll_first(); // removes 2, readptr=2
        rb.push(5); // writeptr=5, index 0
        rb.push(6); // writeptr=6, index 1
        // Logical contents: [3, 4, 5, 6], wrapping around array boundary

        let mut buf = [0i32; 4];
        let n = rb.read(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(buf, [3, 4, 5, 6]);
    }

    #[test]
    fn test_read_is_non_destructive() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(10);
        rb.push(20);

        let mut buf1 = [0i32; 2];
        let mut buf2 = [0i32; 2];
        let n1 = rb.read(&mut buf1);
        let n2 = rb.read(&mut buf2);
        assert_eq!(n1, n2);
        assert_eq!(buf1, buf2);
        assert_eq!(rb.len(), 2);
    }

    #[test]
    fn test_read_into_zero_length_buffer() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        let mut buf: [i32; 0] = [];
        let n = rb.read(&mut buf);
        assert_eq!(n, 0);
    }

    #[test]
    fn test_write_overwrites_existing_content() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        rb.write(&mut [10, 20, 30, 40]);
        assert_eq!(rb.poll_first(), Some(10));
        assert_eq!(rb.poll_first(), Some(20));
        assert_eq!(rb.poll_first(), Some(30));
        assert_eq!(rb.poll_first(), Some(40));
    }

    #[test]
    fn test_write_across_wrap_boundary() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        rb.poll_first(); // readptr=1
        rb.poll_first(); // readptr=2
        rb.push(5);
        rb.push(6);
        // readptr=2, writeptr=6, len=4, wraps around

        rb.write(&mut [10, 20, 30, 40]);
        let mut buf = [0i32; 4];
        rb.read(&mut buf);
        assert_eq!(buf, [10, 20, 30, 40]);
    }

    #[test]
    #[should_panic(expected = "Buffer is too big")]
    fn test_write_panics_when_buf_exceeds_capacity() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.write(&mut [1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_write_partial() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }

        rb.write(&mut [10, 20]);

        // Older elements were overriden.
        assert_eq!(rb.poll_first(), Some(3));
        assert_eq!(rb.poll_first(), Some(4));
        assert_eq!(rb.poll_first(), Some(10));
        assert_eq!(rb.poll_first(), Some(20));
    }

    #[test]
    fn test_drop_first_from_empty() {
        let mut rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.drop_first(3), 0);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_drop_first_fewer_than_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        assert_eq!(rb.drop_first(2), 2);
        assert_eq!(rb.len(), 2);
        assert_eq!(rb.peek_first(), Some(&3));
    }

    #[test]
    fn test_drop_first_exactly_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        assert_eq!(rb.drop_first(4), 4);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_drop_first_more_than_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.drop_first(5), 2);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_drop_first_zero() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        assert_eq!(rb.drop_first(0), 0);
        assert_eq!(rb.len(), 1);
    }

    #[test]
    fn test_drop_last_from_empty() {
        let mut rb = RingBuffer::<i32, 4>::new();
        assert_eq!(rb.drop_last(3), 0);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_drop_last_fewer_than_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        assert_eq!(rb.drop_last(2), 2);
        assert_eq!(rb.len(), 2);
        assert_eq!(rb.peek_last(), Some(&2));
    }

    #[test]
    fn test_drop_last_exactly_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        assert_eq!(rb.drop_last(4), 4);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_drop_last_more_than_available() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.drop_last(5), 2);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_drop_last_zero() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        assert_eq!(rb.drop_last(0), 0);
        assert_eq!(rb.len(), 1);
    }

    #[test]
    fn test_heavy_wrap_around_cycling() {
        let mut rb = RingBuffer::<i32, 4>::new();
        // Push 2, poll 2, repeat 100 times to advance pointers far
        for cycle in 0..100 {
            let base = cycle * 2;
            rb.push(base);
            rb.push(base + 1);
            assert_eq!(rb.poll_first(), Some(base));
            assert_eq!(rb.poll_first(), Some(base + 1));
        }
        assert!(rb.is_empty());

        // Now fill it and verify
        for i in 1..=4 {
            rb.push(i);
        }
        let mut buf = [0i32; 4];
        rb.read(&mut buf);
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    fn test_full_wrap_around_with_overwrite() {
        let mut rb = RingBuffer::<i32, 3>::new();
        for i in 1..=9 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 3);
        assert_eq!(rb.poll_first(), Some(7));
        assert_eq!(rb.poll_first(), Some(8));
        assert_eq!(rb.poll_first(), Some(9));
    }

    #[test]
    fn test_peek_after_wrap() {
        let mut rb = RingBuffer::<i32, 3>::new();
        for i in 1..=5 {
            rb.push(i);
        }
        assert_eq!(rb.peek_first(), Some(&3));
        assert_eq!(rb.peek_last(), Some(&5));
    }

    #[test]
    fn test_push_overwrite_drops_evicted() {
        let counter = Cell::new(0u32);
        let mut rb = RingBuffer::<DropCounter, 3>::new();
        rb.push(DropCounter::new(1, &counter));
        rb.push(DropCounter::new(2, &counter));
        rb.push(DropCounter::new(3, &counter));
        assert_eq!(counter.get(), 0); // nothing dropped yet

        rb.push(DropCounter::new(4, &counter)); // evicts id=1
        assert_eq!(counter.get(), 1);

        rb.push(DropCounter::new(5, &counter)); // evicts id=2
        assert_eq!(counter.get(), 2);
    }

    #[test]
    fn test_poll_first_returns_owned_no_double_drop() {
        let counter = Cell::new(0u32);
        let mut rb = RingBuffer::<DropCounter, 3>::new();
        rb.push(DropCounter::new(1, &counter));
        rb.push(DropCounter::new(2, &counter));

        {
            let val = rb.poll_first().unwrap();
            assert_eq!(val.id, 1);
            assert_eq!(counter.get(), 0); // not yet dropped, we own it
        }
        // val goes out of scope here
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_poll_last_returns_owned_no_double_drop() {
        let counter = Cell::new(0u32);
        let mut rb = RingBuffer::<DropCounter, 3>::new();
        rb.push(DropCounter::new(1, &counter));
        rb.push(DropCounter::new(2, &counter));

        {
            let val = rb.poll_last().unwrap();
            assert_eq!(val.id, 2);
            assert_eq!(counter.get(), 0);
        }
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_drop_first_drops_n_elements() {
        let counter = Cell::new(0u32);
        let mut rb = RingBuffer::<DropCounter, 4>::new();
        for i in 1..=4 {
            rb.push(DropCounter::new(i, &counter));
        }
        assert_eq!(counter.get(), 0);

        rb.drop_first(2);
        assert_eq!(counter.get(), 2);
        assert_eq!(rb.len(), 2);

        // Remaining elements should still be valid
        assert_eq!(rb.peek_first().unwrap().id, 3);
    }

    #[test]
    fn test_drop_last_drops_n_elements() {
        let counter = Cell::new(0u32);
        let mut rb = RingBuffer::<DropCounter, 4>::new();
        for i in 1..=4 {
            rb.push(DropCounter::new(i, &counter));
        }
        assert_eq!(counter.get(), 0);

        rb.drop_last(2);
        assert_eq!(counter.get(), 2);
        assert_eq!(rb.len(), 2);

        // Remaining elements should still be valid
        assert_eq!(rb.peek_last().unwrap().id, 2);
    }

    #[test]
    fn test_drop_counter_consistency_through_lifecycle() {
        let counter = Cell::new(0u32);
        let total_created = 10u32;
        {
            let mut rb = RingBuffer::<DropCounter, 3>::new();
            for i in 1..=total_created {
                rb.push(DropCounter::new(i, &counter));
            }
            // 7 elements were evicted by overwrite (elements 1..=7)
            assert_eq!(counter.get(), 7);

            // Poll remaining 3 elements
            let v1 = rb.poll_first().unwrap();
            let v2 = rb.poll_first().unwrap();
            let v3 = rb.poll_first().unwrap();
            assert_eq!(v1.id, 8);
            assert_eq!(v2.id, 9);
            assert_eq!(v3.id, 10);
            assert_eq!(counter.get(), 7); // not yet dropped since we own them

            // Drop owned values
            drop(v1);
            assert_eq!(counter.get(), 8);
            drop(v2);
            assert_eq!(counter.get(), 9);
            drop(v3);
            assert_eq!(counter.get(), 10);
        }
        assert_eq!(counter.get(), total_created);
    }

    #[test]
    fn test_capacity_1_peek_first_equals_last() {
        let mut rb = RingBuffer::<i32, 1>::new();
        rb.push(42);
        assert_eq!(rb.peek_first(), rb.peek_last());
        assert_eq!(rb.poll_first(), Some(42));
        assert!(rb.is_empty());
    }

    #[test]
    fn test_capacity_1_overwrite() {
        let mut rb = RingBuffer::<i32, 1>::new();
        rb.push(1);
        rb.push(2);
        assert_eq!(rb.len(), 1);
        assert_eq!(rb.peek_first(), Some(&2));
    }

    #[test]
    fn test_capacity_1_read_write() {
        let mut rb = RingBuffer::<i32, 1>::new();
        rb.push(5);

        let mut read_buf = [0i32; 1];
        let n = rb.read(&mut read_buf);
        assert_eq!(n, 1);
        assert_eq!(read_buf, [5]);

        rb.write(&mut [99]);
        let mut read_buf2 = [0i32; 1];
        rb.read(&mut read_buf2);
        assert_eq!(read_buf2, [99]);
    }

    #[test]
    fn test_capacity_1_drop_first_and_last() {
        let mut rb = RingBuffer::<i32, 1>::new();
        rb.push(1);
        assert_eq!(rb.drop_first(1), 1);
        assert!(rb.is_empty());

        rb.push(2);
        assert_eq!(rb.drop_last(1), 1);
        assert!(rb.is_empty());
    }

    #[test]
    fn test_mixed_poll_first_and_poll_last() {
        let mut rb = RingBuffer::<i32, 4>::new();
        rb.push(1);
        rb.push(2);
        rb.push(3);
        rb.push(4);

        assert_eq!(rb.poll_first(), Some(1));
        assert_eq!(rb.poll_last(), Some(4));
        assert_eq!(rb.len(), 2);
        assert_eq!(rb.peek_first(), Some(&2));
        assert_eq!(rb.peek_last(), Some(&3));
    }

    #[test]
    fn test_drop_first_then_push_refills() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        rb.drop_first(2); // drop 1, 2
        rb.push(5);
        rb.push(6);
        assert!(rb.is_full());

        let mut buf = [0i32; 4];
        rb.read(&mut buf);
        assert_eq!(buf, [3, 4, 5, 6]);
    }

    #[test]
    fn test_drop_last_then_push_refills() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 1..=4 {
            rb.push(i);
        }
        rb.drop_last(2); // drop 3, 4
        rb.push(5);
        rb.push(6);
        assert!(rb.is_full());
        assert_eq!(rb.poll_first(), Some(1));
        assert_eq!(rb.poll_first(), Some(2));
        assert_eq!(rb.poll_first(), Some(5));
        assert_eq!(rb.poll_first(), Some(6));
    }

    #[test]
    fn test_read_after_many_overwrites_across_wrap() {
        let mut rb = RingBuffer::<i32, 3>::new();
        // Push 20 elements into a capacity-3 buffer
        for i in 1..=20 {
            rb.push(i);
        }
        let mut buf = [0i32; 3];
        rb.read(&mut buf);
        assert_eq!(buf, [18, 19, 20]);
    }

    #[test]
    fn test_write_then_read_round_trip() {
        let mut rb = RingBuffer::<i32, 4>::new();
        for i in 0..4 {
            rb.push(i);
        }
        rb.write(&mut [100, 200, 300, 400]);
        let mut out = [0i32; 4];
        rb.read(&mut out);
        assert_eq!(out, [100, 200, 300, 400]);
    }

    #[test]
    fn test_peek_mut_first_and_last_same_element_capacity_1() {
        let mut rb = RingBuffer::<i32, 1>::new();
        rb.push(10);
        *rb.peek_first_mut().unwrap() = 77;
        assert_eq!(rb.peek_last(), Some(&77));
    }

    #[test]
    fn test_large_capacity() {
        let mut rb = RingBuffer::<u8, 256>::new();
        for i in 0..=255u8 {
            rb.push(i);
        }
        assert!(rb.is_full());
        assert_eq!(rb.len(), 256);
        assert_eq!(rb.peek_first(), Some(&0));
        assert_eq!(rb.peek_last(), Some(&255));

        // Push one more, wrapping
        rb.push(42);
        assert_eq!(rb.len(), 256);
        assert_eq!(rb.peek_first(), Some(&1));
        assert_eq!(rb.peek_last(), Some(&42));
    }
}
