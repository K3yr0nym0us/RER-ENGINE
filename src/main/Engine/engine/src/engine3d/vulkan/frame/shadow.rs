use super::*;

impl FrameRenderer {
    pub(super) fn shadow_pass(
        &mut self,
        slot: usize,
        input: &FrameInput<'_>,
        pipes: &Pipes,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) -> Result<(), VulkanError> {
        let shadow_image = self.targets.shadow.image;
        transition_depth(
            device,
            cmd,
            shadow_image,
            &mut self.shadow_layout,
            DEPTH_ATTACHMENT,
        );
        let extent = self.targets.shadow.extent;
        begin_depth(device, cmd, self.targets.shadow.view, extent);
        set_viewport(device, cmd, extent);
        self.draw_depth(
            slot,
            input,
            pipes.shadow,
            pipes.shadow_skinned,
            self.layouts[2],
            self.layouts[3],
            device,
            cmd,
        );
        unsafe { device.cmd_end_rendering(cmd) };
        transition_depth(
            device,
            cmd,
            shadow_image,
            &mut self.shadow_layout,
            DEPTH_READ_ONLY,
        );
        Ok(())
    }

    pub(super) fn draw_depth(
        &self,
        slot: usize,
        input: &FrameInput<'_>,
        pipeline: vk::Pipeline,
        skinned_pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        skinned_layout: vk::PipelineLayout,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
    ) {
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[self.sets.scene[slot]],
                &[],
            );
        }
        self.draw_indexed(
            slot,
            input.static_batches.iter().map(|batch| {
                (
                    batch.vertex,
                    batch.index,
                    batch.index_count,
                    batch.instance_count,
                )
            }),
            &self.static_offsets,
            device,
            cmd,
        );
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, skinned_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                skinned_layout,
                0,
                &[
                    self.sets.scene[slot],
                    self.sets.empty,
                    self.sets.joints[slot],
                ],
                &[],
            );
        }
        for (index, batch) in input.skinned_batches.iter().enumerate() {
            let push = push_bytes(self.joint_bases.get(index).copied().unwrap_or(0), 0.0);
            unsafe {
                device.cmd_push_constants(
                    cmd,
                    skinned_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    &push,
                );
            }
            let offset = self.skinned_offsets.get(index).copied().unwrap_or(0);
            draw_one(
                device,
                cmd,
                batch.vertex,
                batch.index,
                batch.index_count,
                batch.instance_count,
                &self.instances[slot],
                offset,
            );
        }
    }
}
