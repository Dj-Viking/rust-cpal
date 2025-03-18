use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};

use ringbuf::{
	traits::{Consumer, Producer, Split},
	HeapRb,
};

const DEVICE_NAME: &str = "sysdefault:CARD=AG06MK2";

fn main() -> Result<(), Box<dyn std::error::Error>> {
	println!("all hosts {:?}\n", cpal::ALL_HOSTS);

	let host_id = cpal::available_hosts().to_vec()[0];
	println!("hostname {:?}", host_id.name());

	let host = cpal::host_from_id(host_id)?;
	let devs = host.devices()?;

	let device = devs.into_iter().find(|d| d.name().unwrap().contains(DEVICE_NAME)).unwrap();

	println!("---------------------------");
	println!("selected device name - {}", device.name()?);
	println!("---------------------------");

	let input_config: cpal::StreamConfig = device.default_input_config()?.into();
	let output_config: cpal::StreamConfig = device.default_output_config()?.into();
	println!("input_config {:#?}", input_config);
	println!("---------------------------");
	println!("output_config {:#?}", output_config);

	// create delay in case input and output are not synced
	const LATENCY: f32 = 32.0;
	
	let latency_frames = (LATENCY * 0.001) * input_config.sample_rate.0 as f32;
	let latency_samples = latency_frames as usize * input_config.channels as usize;
	
	// create ring buffer to share samples
	let ring_buffer = HeapRb::<f32>::new(latency_samples * 2);
	let (mut producer, mut consumer) = ring_buffer.split();

	//initialize sample buffer with 0.0 equal to length of delay
	// ring buffer has 2x as many space as necessary to add latency,
	// so hopefully shouldn't fail
	for _ in 0..latency_samples {
		producer.try_push(0.0).unwrap();
	}

	let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
		let mut output_fell_behind = false;
		println!("got sample data from input {:?}", data.len());
		for &sample in data {
			if producer.try_push(sample).is_err() {
				output_fell_behind = true;
			}
		}
		if output_fell_behind {
			eprintln!("output stream fell behind: try increasing latency");
		}
	};

	let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
		let mut input_fell_behind = false;
		println!("got sample data from output {:?}", data.len());
		for sample in data {
			*sample = match consumer.try_pop() {
				Some(s) => s,
				None => {
					input_fell_behind = true;
					0.0
				}
			};
		}
		if input_fell_behind {
			eprintln!("input stream fell behind: try increasing latency");
		}
	};
	
	// Build the streams
	
	println!("attempt to build both streams with f32 samples and input config \n `{:?}`", input_config);

	let input_stream = device.build_input_stream(&input_config, input_data_fn, err_fn, None)?;
	let output_stream = device.build_output_stream(&output_config, output_data_fn, err_fn, None)?;
	println!("successfully built streams");

	//play the streams
	println!("starting the input and output streams with `{}` ms of latency", LATENCY);

	input_stream.play()?;
	output_stream.play()?;

	println!("playing streams for 100 seconds...");

	std::thread::sleep(std::time::Duration::from_secs(100));

	drop(input_stream);
	drop(output_stream);

	println!("done");

	Ok(())
}

fn err_fn(err: cpal::StreamError) {
	eprintln!("an error occurred on stream: {}", err);
}
