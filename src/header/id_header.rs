/// Trait for codec identification headers
pub trait IdHeader {
    /// The number of output channels
    fn num_output_channels(&self) -> usize;

    /// The sample rate of the original source (may not be available)
    fn input_sample_rate(&self) -> Option<usize>;

    /// The sample rate audio should be decoded at
    fn output_sample_rate(&self) -> usize;
}
