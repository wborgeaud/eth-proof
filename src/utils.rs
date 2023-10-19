use ethers::types::{GethDebugBuiltInTracerType, GethDebugTracerType, GethDebugTracingOptions};
use flexi_logger::Logger;

pub fn init_env_logger() {
    let _ = Logger::try_with_env_or_str("plonky2::util::timing=info")
        .unwrap()
        .start();
}

/// Tracing options for the debug_traceTransaction call.
pub(crate) fn tracing_options() -> GethDebugTracingOptions {
    GethDebugTracingOptions {
        tracer: Some(GethDebugTracerType::BuiltInTracer(
            GethDebugBuiltInTracerType::PreStateTracer,
        )),

        ..GethDebugTracingOptions::default()
    }
}
