use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat};

use ringbuf::{
	traits::{Split},
	traits::producer::Producer,
	traits::consumer::Consumer,
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
	
	println!("supported configs {:#?}", device.supported_input_configs().unwrap().collect::<Vec<_>>().into_iter().find(|s| {
		s.channels() == 2 && s.sample_format() == SampleFormat::F32
	}));

	println!("---------------------------");
	println!("selected device name - {}", device.name()?);
	println!("---------------------------");


	let stream_config: cpal::StreamConfig = cpal::StreamConfig {
		sample_rate: cpal::SampleRate(44100),
		channels: 2,
		// buffer_size: cpal::BufferSize::Fixed(4096)
		buffer_size: cpal::BufferSize::Default
	};
	println!("stream_config {:#?}", stream_config);
	println!("---------------------------");

	// create delay in case input and output are not synced
	const LATENCY: f32 = 50.0;
	
	let latency_frames = (LATENCY * 0.001) * stream_config.sample_rate.0 as f32;
	println!("frames? {}", latency_frames);
	let latency_samples = latency_frames as usize * stream_config.channels as usize;
	println!("samples? {}", latency_samples);
	
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
	
	println!("attempt to build both streams with f32 samples and input config \n `{:?}`", stream_config);

	let input_stream = device.build_input_stream(&stream_config, input_data_fn, err_fn, None)?;
	let output_stream = device.build_output_stream(&stream_config, output_data_fn, err_fn, None)?;
	println!("successfully built streams");

	//play the streams
	println!("starting the input and output streams with `{}` ms of latency", LATENCY);

	let start_stream = || {
		input_stream.play().unwrap();
		output_stream.play().unwrap();

		println!("playing streams for 100 seconds...");

		std::thread::sleep(std::time::Duration::from_secs(100));

		drop(input_stream);
		drop(output_stream);

		println!("done");

	};

	start_stream();

	Ok(())
}

fn err_fn(err: cpal::StreamError) {
	eprintln!("an error occurred on stream: {}", err);
}
