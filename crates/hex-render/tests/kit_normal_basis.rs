//! `kit_common::kit_normal_basis` must be the inverse-transpose of the instance basis.
//!
//! Every kit piece's world normal goes through it (kit_vertex.wesl and kit_prepass.wesl, both the
//! batched and the instanced entry). Returning `inverse(m)` instead is exact at yaw 0 and 180
//! degrees and mirrors the yaw elsewhere, so sun-facing walls of rotated pieces shade black
//! (2026-10-19, house_cottage at yaw 90/270). This runs the function text from the `.wesl` source
//! in a compute shader and compares it against glam. It skips when no GPU adapter exists.
use bevy::math::{Mat3, Vec3};
use wgpu::util::DeviceExt;

const KIT_COMMON: &str = include_str!("../src/kit_common.wesl");

fn extract_fn(src: &str, name: &str) -> String {
    let start = src
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("fn {name} not found in kit_common.wesl"));
    let end = src[start..]
        .find("\n}\n")
        .unwrap_or_else(|| panic!("end of fn {name} not found"));
    src[start..start + end + 2].to_string()
}

fn cases() -> Vec<Mat3> {
    let yaw = |deg: f32| Mat3::from_rotation_y(deg.to_radians());
    vec![
        Mat3::IDENTITY,
        yaw(30.0),
        yaw(90.0),
        yaw(180.0),
        yaw(270.0),
        yaw(90.0) * Mat3::from_diagonal(Vec3::new(2.0, 1.0, 0.5)),
        Mat3::from_rotation_x(0.3) * yaw(45.0) * Mat3::from_diagonal(Vec3::new(1.2, 1.2, 1.2)),
    ]
}

/// Runs `kit_normal_basis` on the GPU for each matrix; `None` when there is no adapter.
fn gpu_normal_basis(mats: &[Mat3]) -> Option<Vec<Mat3>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        bevy::tasks::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) =
        bevy::tasks::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;

    let src = format!(
        "@group(0) @binding(0) var<storage, read> mats: array<vec4<f32>>;\n\
         @group(0) @binding(1) var<storage, read_write> outs: array<vec4<f32>>;\n\
         {}\n\
         @compute @workgroup_size(1)\n\
         fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
             let i = id.x * 3u;\n\
             let n = kit_normal_basis(mat3x3(mats[i].xyz, mats[i + 1u].xyz, mats[i + 2u].xyz));\n\
             outs[i] = vec4(n[0], 0.0);\n\
             outs[i + 1u] = vec4(n[1], 0.0);\n\
             outs[i + 2u] = vec4(n[2], 0.0);\n\
         }}\n",
        extract_fn(KIT_COMMON, "kit_normal_basis")
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("kit_normal_basis"),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("kit_normal_basis"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let input: Vec<f32> = mats
        .iter()
        .flat_map(|m| {
            [m.x_axis, m.y_axis, m.z_axis]
                .into_iter()
                .flat_map(|c| [c.x, c.y, c.z, 0.0])
        })
        .collect();
    let size = (input.len() * 4) as u64;
    let in_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mats"),
        contents: bytemuck::cast_slice(&input),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("outs"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("read"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: in_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: out_buf.as_entire_binding(),
            },
        ],
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(mats.len() as u32, 1, 1);
    }
    enc.copy_buffer_to_buffer(&out_buf, 0, &read_buf, 0, size);
    queue.submit([enc.finish()]);
    read_buf.map_async(wgpu::MapMode::Read, .., |r| r.expect("map read buffer"));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("poll device");
    let view = read_buf.get_mapped_range(..).expect("mapped range");
    let out: &[f32] = bytemuck::cast_slice(&view);
    Some(
        out.as_chunks::<12>()
            .0
            .iter()
            .map(|c| {
                Mat3::from_cols(
                    Vec3::new(c[0], c[1], c[2]),
                    Vec3::new(c[4], c[5], c[6]),
                    Vec3::new(c[8], c[9], c[10]),
                )
            })
            .collect(),
    )
}

#[test]
fn kit_normal_basis_is_inverse_transpose() {
    let mats = cases();
    let Some(got) = gpu_normal_basis(&mats) else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    for (m, g) in mats.iter().zip(&got) {
        let want = m.inverse().transpose();
        assert!(
            g.abs_diff_eq(want, 1e-4),
            "kit_normal_basis({m:?}) = {g:?}, want inverse-transpose {want:?}"
        );
    }
}
