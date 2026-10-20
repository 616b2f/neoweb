fn main() -> iced::Result {
    let args = neoweb::CliArgs::from_env();
    if let Some(token) = args.content_process_token {
        servo::run_content_process(token);
        return Ok(());
    }

    neoweb::run_with_args(args)
}
