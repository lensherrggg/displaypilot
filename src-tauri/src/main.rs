fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = displaypilot_lib::config::AppPaths::discover();

    if let Err(error) = displaypilot_lib::logging::init(&paths.log_dir, &paths.log_file) {
        eprintln!("DisplayPilot 日志初始化失败：{error}");
    }

    if displaypilot_lib::cli::should_run(&args) {
        std::process::exit(displaypilot_lib::cli::run(args, paths));
    }

    displaypilot_lib::run();
}
