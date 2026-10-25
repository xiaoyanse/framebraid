use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("用法：inspect <模型.onnx> <图片.jpg>".into());
    }
    let backend = std::env::var("AM_RT_VISION_BACKEND")
        .ok()
        .and_then(|value| am_vision::parse_backend(&value).ok())
        .unwrap_or_default();
    let mut model = am_vision::PoseModel::load(Path::new(&args[1]), backend)?;
    let image = std::fs::read(&args[2])?;
    let result = model.infer(&image, 0.25)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "model": model.info,
            "execution": model.execution,
            "frame": result
        }))?
    );
    Ok(())
}
