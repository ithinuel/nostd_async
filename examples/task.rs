pub fn main() {
    let runtime = nostd_async::Runtime::new();

    let mut task = nostd_async::Task::new(async {
        println!("Hello World");
        42
    });

    let handle = runtime.spawn(&mut task);

    println!("{}", handle.join());
}
