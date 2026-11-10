const CAPACITY: usize = 4096;

static mut VALUES: [f64; CAPACITY * 2] = [0.0; CAPACITY * 2];

#[no_mangle]
pub extern "C" fn stats_buffer_ptr() -> *mut f64 {
    core::ptr::addr_of_mut!(VALUES).cast()
}

#[no_mangle]
pub extern "C" fn stats_buffer_capacity() -> usize {
    CAPACITY
}

/// Projects the shared chart buffer in place, or returns zero for invalid data.
///
/// # Safety
/// The caller must have exclusive access to the shared buffer for this call.
/// No thread may read or write it, or run another projection, concurrently.
#[no_mangle]
pub unsafe extern "C" fn stats_project_points(
    pointer: usize,
    count: usize,
    start: f64,
    end: f64,
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
    minimum: f64,
    maximum: f64,
) -> usize {
    if pointer != stats_buffer_ptr() as usize
        || count > CAPACITY
        || !start.is_finite()
        || !end.is_finite()
        || end <= start
        || !minimum.is_finite()
        || !maximum.is_finite()
        || maximum <= minimum
        || !(end - start).is_finite()
        || !(maximum - minimum).is_finite()
        || ![left, right, bottom, top].iter().all(|value| value.is_finite())
    {
        return 0;
    }

    let values = unsafe { core::slice::from_raw_parts_mut(pointer as *mut f64, count * 2) };
    // Check every point before writing any output, so rejected data leaves the
    // caller's buffer intact.
    if values.chunks_exact(2).any(|pair| {
        if !pair[0].is_finite() || !pair[1].is_finite() {
            return true;
        }
        let x = left + (right - left) * (pair[0] - start) / (end - start);
        let y = bottom - (bottom - top) * (pair[1] - minimum) / (maximum - minimum);
        !x.is_finite() || !y.is_finite()
    }) {
        return 0;
    }
    for pair in values.chunks_exact_mut(2) {
        let x = left + (right - left) * (pair[0] - start) / (end - start);
        let y = bottom - (bottom - top) * (pair[1] - minimum) / (maximum - minimum);
        pair[0] = x;
        pair[1] = y;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static BUFFER_LOCK: Mutex<()> = Mutex::new(());

    fn project(pointer: usize, count: usize, start: f64, end: f64,
        left: f64, right: f64, bottom: f64, top: f64, minimum: f64, maximum: f64) -> usize {
        // Each test owns BUFFER_LOCK while accessing the shared buffer.
        unsafe { stats_project_points(pointer, count, start, end, left, right, bottom, top, minimum, maximum) }
    }

    #[test]
    fn projects_points_to_the_chart_bounds() {
        let _guard = BUFFER_LOCK.lock().unwrap();
        unsafe {
            let values = &mut *core::ptr::addr_of_mut!(VALUES);
            values[..4].copy_from_slice(&[0.0, 0.0, 10.0, 10.0]);
        }

        assert_eq!(
            project(
                stats_buffer_ptr() as usize,
                2,
                0.0,
                10.0,
                5.0,
                105.0,
                205.0,
                5.0,
                0.0,
                10.0
            ),
            2
        );
        unsafe {
            let values = &*core::ptr::addr_of!(VALUES);
            assert_eq!(&values[..4], &[5.0, 205.0, 105.0, 5.0]);
        }
    }

    #[test]
    fn rejects_invalid_ranges_and_oversized_inputs() {
        let _guard = BUFFER_LOCK.lock().unwrap();
        let pointer = stats_buffer_ptr() as usize;
        assert_eq!(
            project(
                pointer,
                CAPACITY + 1,
                0.0,
                1.0,
                0.0,
                1.0,
                1.0,
                0.0,
                0.0,
                1.0
            ),
            0
        );
        assert_eq!(
            project(pointer, 1, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0),
            0
        );
        assert_eq!(
            project(pointer, 1, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0),
            0
        );
    }

    #[test]
    fn rejects_nonfinite_geometry_without_changing_the_buffer() {
        let _guard = BUFFER_LOCK.lock().unwrap();
        let pointer = stats_buffer_ptr() as usize;
        for geometry in [
            [f64::NAN, 100.0, 200.0, 0.0],
            [0.0, f64::INFINITY, 200.0, 0.0],
            [0.0, 100.0, f64::NAN, 0.0],
            [0.0, 100.0, 200.0, f64::NEG_INFINITY],
            [-f64::MAX, f64::MAX, 200.0, 0.0],
        ] {
            unsafe { (&mut *core::ptr::addr_of_mut!(VALUES))[..2].copy_from_slice(&[0.5, 0.5]); }
            assert_eq!(project(pointer, 1, 0.0, 1.0, geometry[0], geometry[1], geometry[2], geometry[3], 0.0, 1.0), 0);
            unsafe { assert_eq!(&(&*core::ptr::addr_of!(VALUES))[..2], &[0.5, 0.5]); }
        }
    }

    #[test]
    fn rejects_bad_samples_atomically() {
        let _guard = BUFFER_LOCK.lock().unwrap();
        let pointer = stats_buffer_ptr() as usize;
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            unsafe { (&mut *core::ptr::addr_of_mut!(VALUES))[..4].copy_from_slice(&[0.0, 0.0, 1.0, invalid]); }
            assert_eq!(project(pointer, 2, 0.0, 1.0, 5.0, 105.0, 205.0, 5.0, 0.0, 1.0), 0);
            unsafe {
                let values = &*core::ptr::addr_of!(VALUES);
                assert_eq!(&values[..3], &[0.0, 0.0, 1.0]);
                assert_eq!(values[3].to_bits(), invalid.to_bits());
            }
        }
    }
}
