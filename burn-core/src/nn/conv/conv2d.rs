use alloc::{format, vec::Vec};

use crate as burn;

use crate::config::Config;
use crate::module::Module;
use crate::module::Param;
use crate::nn::Initializer;
use crate::tensor::backend::Backend;
use crate::tensor::Tensor;
use burn_tensor::module::conv2d;
use burn_tensor::ops::conv::calculate_padding;

use libm::sqrt;

/// Configuration to create an [2D convolution](Conv2d) layer.
#[derive(Config)]
pub struct Conv2dConfig {
    /// The number of channels.
    pub channels: [usize; 2],
    /// The size of the kernel.
    pub kernel_size: [usize; 2],
    /// The padding configuration.
    #[config(default = "Conv2dPaddingConfig::Valid")]
    pub padding: Conv2dPaddingConfig,
    /// If bias should be added to the output.
    #[config(default = true)]
    pub bias: bool,
    /// The type of function used to initialize neural network parameters
    #[config(default = "Initializer::UniformDefault")]
    pub initializer: Initializer,
}

/// Padding configuration for 2D convolution [config](Conv2dConfig).
#[derive(Config, Debug)]
pub enum Conv2dPaddingConfig {
    /// Dynamicaly calculate the amount of padding necessary to ensure that the output size will be
    /// the same as the input.
    Same,
    /// Same as no padding.
    Valid,
    /// Applies the specified amount of padding to all inputs.
    Explicit(usize, usize),
}

/// Applies a 2D convolution over input tensors.
///
/// # Params
///
/// - weight: Tensor of shape [channels_out, channels_in, kernel_size_1, kernel_size_2] initialized from a uniform
///     distribution `U(-k, k)` where `k = sqrt(1 / channels_in * kernel_size_1 * kernel_size_2)`
///
/// - bias:   Tensor of shape [channels_out], initialized from a uniform distribution `U(-k, k)`
///     where `k = sqrt(1 / channels_in * kernel_size_1 * kernel_size_2)`
#[derive(Module, Debug)]
pub struct Conv2d<B: Backend> {
    weight: Param<Tensor<B, 4>>,
    bias: Param<Option<Tensor<B, 1>>>,
    stride: [usize; 2],
    kernel_size: [usize; 2],
    padding: Conv2dPaddingConfig,
}

impl<B: Backend> Conv2d<B> {
    /// Create the module from the given configuration.
    pub fn new(config: &Conv2dConfig) -> Self {
        let k = (config.channels[0] * config.kernel_size[0] * config.kernel_size[1]) as f64;
        let k = sqrt(1.0 / k);

        let initializer = if let Initializer::UniformDefault = config.initializer {
            Initializer::Uniform(-k, k)
        } else {
            config.initializer.clone()
        };

        let weight = initializer.init([
            config.channels[1],
            config.channels[0],
            config.kernel_size[0],
            config.kernel_size[1],
        ]);

        let bias = if config.bias {
            Some(initializer.init([config.channels[1]]))
        } else {
            None
        };

        Self {
            weight: Param::from(weight),
            bias: Param::from(bias),
            stride: [1, 1], // TODO: Add the stride to the configuration when properly supported.
            kernel_size: config.kernel_size,
            padding: config.padding.clone(),
        }
    }

    /// Applies the forward pass on the input tensor.
    ///
    /// # Shapes
    ///
    /// - input: [batch_size, channels_in, height_in, width_in],
    /// - output: [batch_size, channels_out, height_out, width_out],
    pub fn forward(&self, input: Tensor<B, 4>) -> Tensor<B, 4> {
        let [_batch_size, _channels_in, height_in, width_in] = input.dims();
        let padding =
            self.padding
                .calculate_padding_2d(height_in, width_in, &self.kernel_size, &self.stride);
        conv2d(
            input,
            self.weight.val(),
            self.bias.val(),
            self.stride,
            padding,
        )
    }
}

impl Conv2dPaddingConfig {
    pub(crate) fn calculate_padding_2d(
        &self,
        height: usize,
        width: usize,
        kernel_size: &[usize; 2],
        stride: &[usize; 2],
    ) -> [usize; 2] {
        let same_padding = || {
            let p1 = calculate_padding(kernel_size[0], stride[0], height, height);
            let p2 = calculate_padding(kernel_size[1], stride[1], width, width);

            [p1, p2]
        };

        match self {
            Conv2dPaddingConfig::Same => same_padding(),
            Conv2dPaddingConfig::Valid => [0, 0],
            Conv2dPaddingConfig::Explicit(v1, v2) => [*v1, *v2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub type TB = burn_ndarray::NdArrayBackend<f32>;

    #[test]
    fn initializer_default() {
        TB::seed(0);
        let config = Conv2dConfig::new([5, 1], [5, 5]);
        let k = (config.channels[0] * config.kernel_size[0] * config.kernel_size[1]) as f64;
        let k = sqrt(1.0 / k);
        assert_eq!(config.initializer, Initializer::UniformDefault);
        let conv: Conv2d<TB> = Conv2d::new(&config);
        for item in conv.weight.to_data().value.iter() {
            if *item < -k as f32 || *item > k as f32 {
                panic!("Element ({item}) is not within the range of (-{k},{k})");
            }
        }
    }

    #[test]
    fn initializer_zeros() {
        TB::seed(0);
        let config = Conv2dConfig::new([5, 2], [5, 5]).with_initializer(Initializer::Zeros);
        assert_eq!(config.initializer, Initializer::Zeros);
        let conv: Conv2d<TB> = Conv2d::new(&config);
        for item in conv.weight.to_data().value.iter() {
            assert_eq!(*item, 0.0f32);
        }
    }
}
