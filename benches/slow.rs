use divan::AllocProfiler;
use ccmk4::slow_function;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main(){
	divan::main();
}

#[divan::bench]
fn bench_slow_function(){
	slow_function();
}