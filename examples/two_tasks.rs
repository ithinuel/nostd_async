pub fn main() {
    let runtime = nostd_async::Runtime::new();

    let mut t1 = nostd_async::Task::new(async {
        println!("Hello from Task 1");
        1
    });
    let mut t2 = nostd_async::Task::new(async {
        println!("Hello from Task 2");
        2
    });

    let h1 = runtime.spawn(&mut t1);
    let h2 = runtime.spawn(&mut t2);

    println!("Task 1: {}", h1.join());
    println!("Task 2: {}", h2.join());
}
