//! Isolated held-descriptor caller experiment. No public router uses this example.
#[path = "transcript_caller_trial/authority.rs"]
mod authority;
#[path = "transcript_caller_trial/executor.rs"]
mod executor;
#[path = "transcript_caller_trial/output.rs"]
mod output;
#[path = "transcript_caller_trial/protocol.rs"]
mod protocol;
#[path = "transcript_caller_trial/supervisor.rs"]
mod supervisor;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("trial runtime");
    let result = rt.block_on(async {
        match args.as_slice() {
            [role, p, v] if role == "authority" && p == "--protocol" && v == "1" => {
                authority::run().await
            }
            [role, p, v, j, job]
                if role == "executor"
                    && p == "--protocol"
                    && v == "1"
                    && j == "--job-json"
                    && job.len() <= protocol::MAX_JOB_JSON_BYTES =>
            {
                executor::run(job).await
            }
            _ => Err("bad_request"),
        }
    });
    if result.is_err() {
        std::process::exit(1)
    }
}
