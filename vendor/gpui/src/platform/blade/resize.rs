// Small resize steps should not allocate new multisampled path targets. Keep a
// bounded amount of spare capacity while preserving exact drawable dimensions.
pub(super) fn path_target_extent(viewport: [u32; 2], current: [u32; 2]) -> [u32; 2] {
    std::array::from_fn(|axis| {
        let required = viewport[axis].max(1);
        let allocated = current[axis];
        // Two buckets of hysteresis avoid reallocating when the pointer moves
        // back across a bucket boundary. A large shrink must release the extra
        // MSAA pixels rather than clearing a formerly maximized target forever.
        if required <= allocated && allocated - required < 128 {
            allocated
        } else if required <= 4096 {
            // 4096 is within the required Vulkan 2D-image limit. Larger targets
            // retain the requested extent instead of guessing the GPU's limit.
            required.div_ceil(64) * 64
        } else {
            required
        }
    })
}

#[cfg(test)]
mod tests {
    use super::path_target_extent;

    #[test]
    fn continuous_resize_reuses_capacity_in_both_directions() {
        let mut current = path_target_extent([1000, 700], [0, 0]);
        let mut allocations = 1;
        for width in (1001..=1400).chain((1000..1400).rev()) {
            let next = path_target_extent([width, 700], current);
            assert!(next[0] >= width && next[1] >= 700);
            assert!(next[0] - width < 128 && next[1] - 700 < 128);
            if next != current {
                allocations += 1;
                current = next;
            }
        }
        // Exact-size targets would allocate for every one of these 801 sizes.
        assert_eq!(allocations, 10);
    }

    #[test]
    fn large_shrinks_release_spare_capacity_per_axis() {
        assert_eq!(path_target_extent([1000, 1600], [2048, 1664]), [1024, 1664]);
        assert_eq!(path_target_extent([1024, 800], [2048, 1664]), [1024, 832]);
    }

    #[test]
    fn zero_dimensions_and_large_targets_remain_valid() {
        assert_eq!(path_target_extent([0, 0], [0, 0]), [64, 64]);
        assert_eq!(path_target_extent([4096, 4097], [0, 0]), [4096, 4097]);
        assert_eq!(path_target_extent([u32::MAX, 1], [0, 0]), [u32::MAX, 64]);
    }
}
