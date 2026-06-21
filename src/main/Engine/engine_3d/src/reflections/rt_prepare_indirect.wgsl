struct DispatchIndirect {
    x : u32,
    y : u32,
    z : u32,
}

@group(0) @binding(0) var<storage, read_write> tile_count : atomic<u32>;
@group(0) @binding(1) var<storage, read_write> indirect : DispatchIndirect;

@compute @workgroup_size(1, 1, 1)
fn cs_prepare_indirect() {
    let c = atomicLoad(&tile_count);
    indirect.x = c;
    indirect.y = 1u;
    indirect.z = 1u;
}
