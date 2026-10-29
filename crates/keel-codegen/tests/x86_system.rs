//! x86 system, privileged and port I/O instruction tests. Generated code never executes here.
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
#[path = "x86_system/rflags_region_prerequisite.rs"]
mod rflags_region_prerequisite;
#[allow(dead_code)]
#[path = "common/userspace.rs"]
mod userspace;
#[path = "x86_system/x86_cpu_control.rs"]
mod x86_cpu_control;
#[path = "x86_system/x86_descriptor_cpl0.rs"]
mod x86_descriptor_cpl0;
#[path = "x86_system/x86_descriptor_queries_encoding.rs"]
mod x86_descriptor_queries_encoding;
#[path = "x86_system/x86_interrupt_control.rs"]
mod x86_interrupt_control;
#[path = "x86_system/x86_iretq.rs"]
mod x86_iretq;
#[path = "x86_system/x86_kernel_code_model.rs"]
mod x86_kernel_code_model;
#[path = "x86_system/x86_msr.rs"]
mod x86_msr;
#[path = "x86_system/x86_port_io.rs"]
mod x86_port_io;
#[path = "x86_system/x86_privileged_control.rs"]
mod x86_privileged_control;
#[path = "x86_system/x86_string_port_io.rs"]
mod x86_string_port_io;
#[path = "x86_system/x86_swapgs.rs"]
mod x86_swapgs;
#[path = "x86_system/x86_system_registers.rs"]
mod x86_system_registers;
#[path = "x86_system/x86_system_write.rs"]
mod x86_system_write;
#[path = "x86_system/x86_xgetbv.rs"]
mod x86_xgetbv;
