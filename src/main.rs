use clap::{Parser, Subcommand, ValueEnum};
use stopwatch::{workloads, Profiler, SystemClock};

#[derive(Parser)]
#[command(name = "stopwatch", about = "A small profiler that measures where a program's time goes")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a built-in workload under the profiler.
    Run {
        #[arg(long, value_enum)]
        workload: Workload,
        /// Print only the JSON call tree, no text report.
        #[arg(long)]
        json: bool,
        /// Print folded stacks for flamegraph.pl instead of the report.
        #[arg(long)]
        flamegraph: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Workload {
    Fib,
    Sort,
    Blur,
    Mixed,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { workload, json, flamegraph } => run_workload(workload, json, flamegraph),
    }
}

fn run_workload(workload: Workload, json: bool, flamegraph: bool) {
    let mut profiler = Profiler::new(SystemClock::new());

    let outcome = match workload {
        Workload::Fib => workloads::fib(&mut profiler, 27).map(|_| ()),
        Workload::Sort => workloads::sort(&mut profiler),
        Workload::Blur => workloads::blur(&mut profiler).map(|_| ()),
        Workload::Mixed => workloads::mixed(&mut profiler),
    };

    if let Err(err) = outcome {
        eprintln!("stopwatch: {err}");
        std::process::exit(1);
    }

    if flamegraph {
        print!("{}", profiler.to_flamegraph());
    } else if json {
        println!("{}", profiler.to_json());
    } else {
        println!("{}", profiler.text_report());
        if let Some(epitaph) = profiler.epitaph() {
            println!("{}", epitaph.message());
        }
    }
}
