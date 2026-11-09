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

#[no_mangle]
pub extern "C" fn stats_project_points(
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
    {
        return 0;
    }

    let values = unsafe { core::slice::from_raw_parts_mut(pointer as *mut f64, count * 2) };
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

    #[test]
    fn projects_points_to_the_chart_bounds() {
        unsafe {
            let values = &mut *core::ptr::addr_of_mut!(VALUES);
            values[..4].copy_from_slice(&[0.0, 0.0, 10.0, 10.0]);
        }

        assert_eq!(
            stats_project_points(
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
        let pointer = stats_buffer_ptr() as usize;
        assert_eq!(
            stats_project_points(
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
            stats_project_points(pointer, 1, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0),
            0
        );
        assert_eq!(
            stats_project_points(pointer, 1, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0),
            0
        );
    }
}
