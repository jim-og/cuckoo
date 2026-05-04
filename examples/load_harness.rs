// Load harness for the cuckoo timer service.
//
// Usage:
//   cargo run --release --example load_harness -- [OPTIONS]
//
// Important: run with --release. Debug builds will be significantly slower
// and will give misleading results.
//
// Examples:
//   # Calibration — verify generator has headroom before trusting results
//   cargo run --release --example load_harness -- --calibrate --rate 5000
//
//   # Full load test (1000 req/s for 30s steady-state)
//   cargo run --release --example load_harness -- --rate 1000 --duration 30 --interval-ms 500
//
//   # Find saturation point — sweep rates until metrics degrade
//   cargo run --release --example load_harness -- --rate 5000 --duration 30

use async_trait::async_trait;
use clap::Parser;
use cuckoo::{
    core::{Clock, SystemClock},
    infra::App,
    utils::{HttpRequest, HttpResponse, LogLevel, Logger, RouteHandler, Router, full, run_server},
};
use hdrhistogram::Histogram;
use http::{Method, Response};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(about = "Open-loop load harness for the cuckoo timer service")]
struct Config {
    /// Target insert rate (requests per second)
    #[arg(long, default_value_t = 1000)]
    rate: u64,

    /// Steady-state measurement window (seconds, excluding 5s warmup)
    #[arg(long, default_value_t = 30)]
    duration: u64,

    /// Timer interval in milliseconds — how long each timer should sleep before firing
    #[arg(long, default_value_t = 500)]
    interval_ms: u64,

    /// Run calibration: drive GET / at 2× rate to verify generator headroom, then exit
    #[arg(long)]
    calibrate: bool,

    /// Port to run the timer service on
    #[arg(long, default_value_t = 6100)]
    port: u16,

    /// Port for the callback receiver
    #[arg(long, default_value_t = 6101)]
    callback_port: u16,
}

// ── Silent logger (suppresses SUT noise during benchmarking) ─────────────────

struct NoopLogger;

impl Logger for NoopLogger {
    fn log(&self, _level: LogLevel, _message: &str) {}
}

// ── Callback receiver ─────────────────────────────────────────────────────────

// Each callback from the SUT is POST /callback with body { timer_id, pop_time_ms }.
// The handler records (pop_time_ms, received_ms) for jitter computation.
struct CallbackHandler {
    sender: mpsc::Sender<(u64, u64)>,
}

#[async_trait]
impl RouteHandler for CallbackHandler {
    async fn handle(&self, req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
        let received_ms = now_ms();
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&req.body)
            && let Some(pop_time_ms) = v.get("pop_time_ms").and_then(|v| v.as_u64())
        {
            let _ = self.sender.try_send((pop_time_ms, received_ms));
        }
        Ok(Response::new(full("OK")))
    }
}

async fn start_callback_server(
    port: u16,
    sender: mpsc::Sender<(u64, u64)>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let router = Arc::new(Router::new().add(Method::POST, "/callback", CallbackHandler { sender }));
    let (ready_tx, ready_rx) = oneshot::channel();
    let logger: Arc<dyn Logger> = Arc::new(NoopLogger);
    tokio::spawn(run_server(router, logger, port, ready_tx, shutdown));
    ready_rx.await.unwrap();
}

// ── Calibration mode ──────────────────────────────────────────────────────────

async fn calibrate(client: &reqwest::Client, sut_url: &str, rate: u64) {
    let target_rate = rate * 2;
    let duration_secs = 5u64;
    let tick = Duration::from_nanos(1_000_000_000 / target_rate);
    let mut interval = tokio::time::interval(tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let url = format!("{}/", sut_url);
    let start = Instant::now();
    let mut completed = 0u64;
    let deadline = start + Duration::from_secs(duration_secs);

    while Instant::now() < deadline {
        interval.tick().await;
        let c = client.clone();
        let u = url.clone();
        tokio::spawn(async move {
            let _ = c.get(&u).send().await;
        });
        completed += 1;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let achieved_rps = completed as f64 / elapsed;

    println!(
        "=== Calibration (target: {} req/s against GET /) ===",
        target_rate
    );
    println!("  Requests dispatched : {completed}");
    println!("  Elapsed             : {elapsed:.2}s");
    println!("  Achieved rate       : {achieved_rps:.0} req/s");
    if achieved_rps >= target_rate as f64 * 0.9 {
        println!("  Result: PASS — generator can sustain target rate");
    } else {
        println!(
            "  Result: FAIL — generator only achieved {:.0}% of target ({:.0} req/s).",
            100.0 * achieved_rps / target_rate as f64,
            achieved_rps
        );
        println!(
            "          Lower --rate or investigate generator bottleneck before trusting results."
        );
    }
}

// ── Report ────────────────────────────────────────────────────────────────────

fn print_histogram(name: &str, h: &Histogram<u64>, unit: &str) {
    if h.is_empty() {
        println!("  {name}: no samples");
        return;
    }
    println!("  {name} ({unit}):");
    println!("    min   = {}", h.min());
    println!("    p50   = {}", h.value_at_quantile(0.50));
    println!("    p95   = {}", h.value_at_quantile(0.95));
    println!("    p99   = {}", h.value_at_quantile(0.99));
    println!("    p99.9 = {}", h.value_at_quantile(0.999));
    println!("    max   = {}", h.max());
    println!("    count = {}", h.len());
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cfg = Config::parse();

    // Start the SUT.
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let logger: Arc<dyn Logger> = Arc::new(NoopLogger);
    let (term_tx, term_rx) = oneshot::channel::<()>();
    let (ready_tx, ready_rx) = oneshot::channel::<()>();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let port = cfg.port;

    {
        let app_clock = clock.clone();
        let app_logger = logger.clone();
        tokio::spawn(async move {
            let mut app = App::new(app_logger, app_clock, port, async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("Failed to start App");
            app.run(term_rx, ready_tx).await.expect("App run failed");
        });
    }
    ready_rx.await.expect("App failed to signal readiness");

    let sut_url = format!("http://127.0.0.1:{}", cfg.port);

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(512)
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    if cfg.calibrate {
        calibrate(&client, &sut_url, cfg.rate).await;
        let _ = term_tx.send(());
        let _ = shutdown_tx.send(());
        return;
    }

    // Start callback server.
    let (cb_tx, cb_rx) = mpsc::channel::<(u64, u64)>(65536);
    let (cb_shutdown_tx, cb_shutdown_rx) = oneshot::channel::<()>();
    start_callback_server(cfg.callback_port, cb_tx, async {
        let _ = cb_shutdown_rx.await;
    })
    .await;

    let callback_url = format!("http://127.0.0.1:{}/callback", cfg.callback_port);

    // Shared histograms: insert latency in µs, jitter in ms.
    let insert_hist = Arc::new(Mutex::new(Histogram::<u64>::new(4).unwrap()));
    let jitter_hist = Arc::new(Mutex::new(Histogram::<u64>::new(4).unwrap()));

    let warmup_secs = 5u64;
    let total_secs = warmup_secs + cfg.duration;
    let tick = Duration::from_nanos(1_000_000_000 / cfg.rate.max(1));

    println!("=== Cuckoo Load Harness ===");
    println!("  Rate:         {} req/s (open-loop)", cfg.rate);
    println!(
        "  Duration:     {}s + {}s warmup",
        cfg.duration, warmup_secs
    );
    println!("  Timer interval: {}ms", cfg.interval_ms);
    println!("  SUT:          {sut_url}");
    println!("  Callback:     {callback_url}");
    println!();
    println!(
        "Note: first {warmup_secs}s are discarded as warmup. Run with --calibrate first to verify generator headroom."
    );
    println!("      Monitor SUT CPU separately: top -pid $(pgrep -f 'load_harness')");
    println!();

    let run_start = Instant::now();
    let warmup_end = run_start + Duration::from_secs(warmup_secs);
    let load_end = run_start + Duration::from_secs(total_secs);

    // Generator task.
    let insert_hist_gen = insert_hist.clone();
    let client_gen = client.clone();
    let sut_url_gen = sut_url.clone();
    let callback_url_gen = callback_url.clone();
    let interval_ms = cfg.interval_ms;

    let gen_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        while Instant::now() < load_end {
            ticker.tick().await;
            let in_warmup = Instant::now() < warmup_end;
            let c = client_gen.clone();
            let url = sut_url_gen.clone();
            let cb_url = callback_url_gen.clone();
            let hist = insert_hist_gen.clone();

            tokio::spawn(async move {
                let payload = serde_json::json!({
                    "interval_ms": interval_ms,
                    "callback_url": cb_url,
                });
                let t0 = now_us();
                if let Ok(resp) = c.post(format!("{url}/timer")).json(&payload).send().await
                    && resp.status().is_success()
                    && !in_warmup
                {
                    let latency_us = now_us().saturating_sub(t0);
                    if let Ok(mut h) = hist.lock() {
                        let _ = h.record(latency_us);
                    }
                }
            });
        }
    });

    // Stats sampler task.
    let client_stats = client.clone();
    let stats_url = format!("{sut_url}/stats");
    let stats_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        while Instant::now() < load_end {
            ticker.tick().await;
            let elapsed = run_start.elapsed().as_secs_f64();
            let phase = if Instant::now() < warmup_end {
                "WARMUP"
            } else {
                "LOAD  "
            };
            match client_stats.get(&stats_url).send().await {
                Ok(r) if r.status().is_success() => {
                    if let Ok(body) = r.text().await {
                        println!("[{elapsed:6.1}s {phase}] {body}");
                    }
                }
                _ => println!("[{elapsed:6.1}s {phase}] /stats unavailable"),
            }
        }
    });

    // Callback consumer task.
    let jitter_hist_cb = jitter_hist.clone();
    let mut cb_rx = cb_rx;
    let cb_handle = tokio::spawn(async move {
        while Instant::now() < load_end {
            match tokio::time::timeout(Duration::from_millis(100), cb_rx.recv()).await {
                Ok(Some((pop_time_ms, received_ms))) => {
                    let past_warmup = Instant::now() >= warmup_end;
                    if past_warmup {
                        let jitter_ms = received_ms.saturating_sub(pop_time_ms);
                        if let Ok(mut h) = jitter_hist_cb.lock() {
                            let _ = h.record(jitter_ms);
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => {}
            }
        }
        // Drain remaining callbacks after load_end.
        tokio::time::sleep(Duration::from_millis(interval_ms + 2000)).await;
        while let Ok((pop_time_ms, received_ms)) = cb_rx.try_recv() {
            let jitter_ms = received_ms.saturating_sub(pop_time_ms);
            if let Ok(mut h) = jitter_hist_cb.lock() {
                let _ = h.record(jitter_ms);
            }
        }
    });

    gen_handle.await.unwrap();
    stats_handle.await.unwrap();
    cb_handle.await.unwrap();

    // Report.
    println!();
    println!("=== Results (steady-state, {}s window) ===", cfg.duration);
    println!();

    {
        let h = insert_hist.lock().unwrap();
        print_histogram("Insert latency (POST /timer → 200 OK)", &h, "µs");
    }
    println!();
    {
        let h = jitter_hist.lock().unwrap();
        print_histogram(
            "Firing jitter (callback_received_ms - expected_pop_time_ms)",
            &h,
            "ms",
        );
    }

    println!();
    println!("Interpretation:");
    println!("  - If insert latency p99 > 50ms: HTTP layer or event channel is saturated.");
    println!(
        "  - If jitter p99 > 10ms: event-handler poll interval or callback path is the bottleneck."
    );
    println!(
        "  - Check /stats channel_capacity_remaining → 0 means a channel is the first bottleneck."
    );
    println!("  - Re-run at higher --rate until a metric degrades to find the saturation point.");

    let _ = term_tx.send(());
    let _ = shutdown_tx.send(());
    let _ = cb_shutdown_tx.send(());
}
