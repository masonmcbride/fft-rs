mod ffmpegwav;

use std::fs::File;
use plotters::prelude::*;
use rustfft::{FftPlanner, num_complex::Complex};
use ffmpegwav::FfmpegWavFile;

const GRAY: RGBColor = RGBColor(128, 128, 128);

fn main() {
    let mut file = File::open("440hz.wav").expect("File could not be opened");
    let wav_file = FfmpegWavFile::parse(&mut file).expect("Failed to parse WAV file");
    let downsampled_samples: Vec<f32> = wav_file.to_normalized_samples()
        .iter().step_by(16).cloned().collect();
    plot_waveform(&downsampled_samples, "waveform.png").expect("Failed to plot waveform");
    println!("Waveform plot saved to 'waveform.png'");
    plot_fft(&downsampled_samples, wav_file.fmt.sample_rate, "fft_spectrum.png").expect("Failed to plot FFT spectrum");
    println!("FFT spectrum plot saved to 'fft_spectrum.png'");
}

fn plot_waveform(samples: &[f32], output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Define the dimensions of the plot
    let root_area = BitMapBackend::new(output_path, (1280, 720)).into_drawing_area();
    root_area.fill(&WHITE)?;

    // Create a chart context
    let mut chart = ChartBuilder::on(&root_area)
        .caption("Audio Waveform", ("sans-serif", 40).into_font())
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0..samples.len(), -1.0f32..1.0f32)?;

    // Configure the mesh (axes)
    chart
        .configure_mesh()
        .x_desc("Sample Index")
        .y_desc("Amplitude")
        .axis_desc_style(("sans-serif", 30))
        .draw()?;

    // Prepare the data as plot points
    let plot_points: Vec<(usize, f32)> = samples.iter().enumerate().map(|(i, &y)| (i, y)).collect();

    // Draw the waveform line
    chart.draw_series(LineSeries::new(
        plot_points,
        &BLUE, // Waveform color
    ))?
    .label("Waveform")
    .legend(|(x, y)| PathElement::new([(x, y), (x + 20, y)], &BLUE));

    // Draw the legend
    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    Ok(())
}

fn plot_fft(samples: &[f32], sample_rate: u32, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Determine FFT size (next power of two)
    let fft_size = samples.len().next_power_of_two();
    println!("FFT Size: {}", fft_size);

    // Step 2: Prepare input for FFT (pad with zeros if necessary)
    let mut input: Vec<Complex<f32>> = samples.iter()
        .cloned()
        .map(|s| Complex{ re: s, im: 0.0 })
        .collect();
    input.resize(fft_size, Complex{ re: 0.0, im: 0.0 });

    // Step 3: Perform FFT
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    fft.process(&mut input);

    // Step 4: Compute magnitude spectrum
    let magnitudes: Vec<f32> = input.iter()
        .take(fft_size / 2) // Only need the first half (Nyquist)
        .map(|c| c.norm())
        .collect();

    // Step 5: Map FFT bins to frequencies
    let freq_resolution = sample_rate as f32 / fft_size as f32;
    let frequencies: Vec<f32> = (0..magnitudes.len())
        .map(|i| i as f32 * freq_resolution)
        .collect();

    // Step 6: Identify top 5 frequencies
    let mut freq_magnitude_map: Vec<(f32, f32)> = frequencies.iter()
        .cloned()
        .zip(magnitudes.iter().cloned())
        .collect();

    // Sort by magnitude descending
    freq_magnitude_map.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Select top 5 unique frequencies
    let top_five: Vec<(f32, f32)> = freq_magnitude_map.into_iter()
        .filter(|&(_, magnitude)| magnitude > 0.0)
        .take(5)
        .collect();

    println!("Top 5 Frequencies:");
    for (freq, mag) in &top_five {
        println!("Frequency: {:.2} Hz, Magnitude: {:.4}", freq, mag);
    }

    // Step 7: Plot the FFT magnitude spectrum
    plot_fft_spectrum(&frequencies, &magnitudes, &top_five, output_path)?;

    Ok(())
}

/// Plots the FFT magnitude spectrum, highlights the top 5 frequencies, and labels them.
///
/// # Arguments
///
/// * `frequencies` - A slice of frequencies corresponding to FFT bins.
/// * `magnitudes` - A slice of magnitudes corresponding to FFT bins.
/// * `top_five` - A slice of tuples containing the top 5 frequencies and their magnitudes.
/// * `output_path` - The file path where the FFT plot image will be saved.
fn plot_fft_spectrum(frequencies: &[f32], magnitudes: &[f32], top_five: &[(f32, f32)], output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Define the dimensions of the plot (High resolution for better quality)
    let root_area = BitMapBackend::new(output_path, (1920, 1080)).into_drawing_area();
    root_area.fill(&WHITE)?;

    // Determine the maximum magnitude for y-axis scaling
    let max_magnitude = magnitudes.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    // Alternatively, use logarithmic scaling if desired

    // Create a chart context
    let mut chart = ChartBuilder::on(&root_area)
        .caption("FFT Magnitude Spectrum", ("sans-serif", 40).into_font())
        .margin(20)
        .x_label_area_size(80)
        .y_label_area_size(60)
        .build_cartesian_2d(0f32..frequencies.last().cloned().unwrap_or(0.0), 0f32..max_magnitude)?;

    // Configure the mesh (axes) to eliminate extra padding
    chart
        .configure_mesh()
        .disable_mesh() // Disable grid lines for cleaner look
        .x_desc("Frequency (Hz)")
        .y_desc("Magnitude")
        .axis_desc_style(("sans-serif", 30))
        .light_line_style(&GRAY.mix(0.3))
        .draw()?;

    // Prepare the data as plot points
    let plot_points: Vec<(f32, f32)> = frequencies.iter()
        .cloned()
        .zip(magnitudes.iter().cloned())
        .collect();

    // Draw the FFT magnitude line
    chart.draw_series(LineSeries::new(
        plot_points,
        RGBColor(255, 0, 0).stroke_width(2), // Red color with stroke width 2
    ))?
    .label("Magnitude")
    .legend(|(x, y)| PathElement::new([(x, y), (x + 20, y)], RGBColor(255, 0, 0)));

    // Highlight and label the top 5 frequencies
    for &(freq, mag) in top_five {
        // Draw a blue vertical line at the top frequency
        chart.draw_series(LineSeries::new(
            vec![(freq, 0.0), (freq, mag)],
            RGBColor(0, 0, 255).stroke_width(2), // Blue color with stroke width 2
        ))?
        .label(format!("{:.1} Hz", freq))
        .legend(|(x, y)| PathElement::new([(x, y), (x + 20, y)], RGBColor(0, 0, 255)));

        // Add text labels for top frequencies at the bottom
        chart.draw_series(vec![
            Text::new(
                format!("{:.1} Hz", freq),
                (freq, 0.0), // Position at the bottom of the plot
                ("sans-serif", 20).into_font().color(&BLACK),
            )
        ])?;
    }

    // Draw the legend
    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .position(SeriesLabelPosition::UpperLeft)
        .draw()?;

    Ok(())
}