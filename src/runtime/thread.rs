/// Vajra Runtime Thread — OS-level thread primitives without pthreads/C runtime

pub fn spawn<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::spawn(f);
}
