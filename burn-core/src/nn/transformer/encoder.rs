use alloc::{format, vec::Vec};
use burn_tensor::Bool;

use crate::{
    self as burn,
    nn::{attention::MHAAutoregressiveCache, cache::TensorCache},
};

use super::{PositionWiseFeedForward, PositionWiseFeedForwardConfig};
use crate::{
    config::Config,
    module::{Module, Param},
    nn::{
        attention::{MhaInput, MultiHeadAttention, MultiHeadAttentionConfig},
        Dropout, DropoutConfig, LayerNorm, LayerNormConfig,
    },
    tensor::{backend::Backend, Tensor},
};

/// Configuration to create a [Transformer Encoder](TransformerEncoder) layer.
#[derive(Config)]
pub struct TransformerEncoderConfig {
    /// The size of the model.
    pub d_model: usize,
    /// The size of the position-wise feed-forward network.
    pub d_ff: usize,
    /// The number of attention heads.
    pub n_heads: usize,
    /// The number of layers.
    pub n_layers: usize,
    /// The dropout rate. Default: 0.1
    #[config(default = 0.1)]
    pub dropout: f64,
    /// Layer norm will be applied first instead of after the other modules.
    #[config(default = false)]
    pub norm_first: bool,
}

/// The transformer encoder module as describe in the paper [Attention Is All You Need](https://arxiv.org/abs/1706.03762).
///
/// # Params
///
/// - layers: transformer encoder layers with `d_model` input and output features.
#[derive(Module, Debug)]
pub struct TransformerEncoder<B: Backend> {
    layers: Param<Vec<TransformerEncoderLayer<B>>>,
}

/// [Transformer Encoder](TransformerEncoder) forward pass input argument.
#[derive(Debug)]
pub struct TransformerEncoderInput<B: Backend> {
    tensor: Tensor<B, 3>,
    mask_pad: Option<Tensor<B, 2, Bool>>,
    mask_attn: Option<Tensor<B, 3, Bool>>,
}

impl<B: Backend> TransformerEncoderInput<B> {
    /// Create a [transformer encoder](TransformerEncoder) input argument.
    pub fn new(tensor: Tensor<B, 3>) -> Self {
        Self {
            tensor,
            mask_pad: None,
            mask_attn: None,
        }
    }

    /// Register the padding mask.
    pub fn mask_pad(mut self, mask_pad: Tensor<B, 2, Bool>) -> Self {
        self.mask_pad = Some(mask_pad);
        self
    }

    /// Register the attention mask.
    pub fn mask_attn(mut self, mask_attn: Tensor<B, 3, Bool>) -> Self {
        self.mask_attn = Some(mask_attn);
        self
    }
}

impl<B: Backend> TransformerEncoder<B> {
    /// Create the module from the given configuration.
    pub fn new(config: &TransformerEncoderConfig) -> Self {
        let layers = (0..config.n_layers)
            .map(|_| TransformerEncoderLayer::new(config))
            .collect::<Vec<_>>();

        Self {
            layers: Param::from(layers),
        }
    }

    /// Applies the forward pass on the input tensor.
    ///
    /// # Shapes
    ///
    /// - tensor: `[batch_size, seq_length, d_model]`
    /// - output: `[batch_size, seq_length, d_model]`
    pub fn forward(&self, input: TransformerEncoderInput<B>) -> Tensor<B, 3> {
        let mut x = input.tensor;

        for layer in self.layers.iter() {
            x = layer.forward(x, input.mask_pad.clone(), input.mask_attn.clone());
        }

        x
    }
    /// Applies the forward pass on the input tensor using autoregressive cache.
    ///
    /// # Shapes
    ///
    /// - tensor: `[batch_size, seq_length, d_model]`
    /// - output: `[batch_size, seq_length, d_model]`
    pub fn forward_autoregressive_inference(
        &self,
        input: TransformerEncoderInput<B>,
        cache: &mut TransformerEncoderAutoregressiveCache<B>,
    ) -> Tensor<B, 3> {
        let mut x = input.tensor;

        for i in 0..self.layers.len() {
            let layer = self.layers.get(i).unwrap();
            let cache = cache.layers.get_mut(i).unwrap();

            x = layer.forward_autoregressive_inference(
                x,
                input.mask_pad.clone(),
                input.mask_attn.clone(),
                cache,
            );
        }

        x
    }

    /// Create an empty autoregressive cache.
    pub fn new_autoregressive_cache(&self) -> TransformerEncoderAutoregressiveCache<B> {
        TransformerEncoderAutoregressiveCache::empty(self.layers.len())
    }
}

#[derive(Module, Debug)]
struct TransformerEncoderLayer<B: Backend> {
    mha: Param<MultiHeadAttention<B>>,
    pwff: Param<PositionWiseFeedForward<B>>,
    norm_1: Param<LayerNorm<B>>,
    norm_2: Param<LayerNorm<B>>,
    dropout: Dropout,
    norm_first: bool,
}

impl<B: Backend> TransformerEncoderLayer<B> {
    fn new(config: &TransformerEncoderConfig) -> Self {
        let config_norm = LayerNormConfig::new(config.d_model);
        let config_dropout = DropoutConfig::new(config.dropout);
        let config_mha = MultiHeadAttentionConfig::new(config.d_model, config.n_heads)
            .with_dropout(config.dropout);
        let config_pwff = PositionWiseFeedForwardConfig::new(config.d_model, config.d_ff)
            .with_dropout(config.dropout);

        let mha = MultiHeadAttention::new(&config_mha);
        let norm_1 = LayerNorm::new(&config_norm);
        let norm_2 = LayerNorm::new(&config_norm);
        let dropout = Dropout::new(&config_dropout);
        let pwff = PositionWiseFeedForward::new(&config_pwff);

        Self {
            mha: Param::from(mha),
            norm_1: Param::from(norm_1),
            norm_2: Param::from(norm_2),
            pwff: Param::from(pwff),
            dropout,
            norm_first: config.norm_first,
        }
    }

    fn forward(
        &self,
        mut input: Tensor<B, 3>,
        mask_pad: Option<Tensor<B, 2, Bool>>,
        mask_attn: Option<Tensor<B, 3, Bool>>,
    ) -> Tensor<B, 3> {
        if self.norm_first {
            input = self.norm_2.forward(input)
        }

        let mut input_mhs = MhaInput::self_attn(input.clone());

        if let Some(mask_pad) = mask_pad {
            input_mhs = input_mhs.mask_pad(mask_pad);
        }

        if let Some(mask_attn) = mask_attn {
            input_mhs = input_mhs.mask_attn(mask_attn);
        }

        let x_1 = self.mha.forward(input_mhs);
        let x_1 = self.dropout.forward(x_1.context) + input;
        let x_1 = self.norm_1.forward(x_1);

        let x_2 = self.pwff.forward(x_1.clone());
        let mut x_2 = self.dropout.forward(x_2) + x_1;

        if !self.norm_first {
            x_2 = self.norm_2.forward(x_2)
        }

        x_2
    }

    fn forward_autoregressive_inference(
        &self,
        mut input: Tensor<B, 3>,
        mask_pad: Option<Tensor<B, 2, Bool>>,
        mask_attn: Option<Tensor<B, 3, Bool>>,
        cache: &mut TransformerEncoderLayerAutoregressiveCache<B>,
    ) -> Tensor<B, 3> {
        if self.norm_first {
            input = cache
                .norm_2
                .forward_autoregressive(input, 1, |input| self.norm_2.forward(input));
        }

        let mut input_mhs = MhaInput::self_attn(input.clone());

        if let Some(mask_pad) = mask_pad {
            input_mhs = input_mhs.mask_pad(mask_pad);
        }

        if let Some(mask_attn) = mask_attn {
            input_mhs = input_mhs.mask_attn(mask_attn);
        }

        let x_1 = self
            .mha
            .forward_autoregressive_inference(input_mhs, &mut cache.mha);
        let x_1 = self.dropout.forward(x_1.context) + input;
        let x_1 = cache
            .norm_1
            .forward_autoregressive(x_1, 1, |x_1| self.norm_1.forward(x_1));

        let x_2 = cache
            .pwff
            .forward_autoregressive(x_1.clone(), 1, |x_1| self.pwff.forward(x_1));
        let mut x_2 = self.dropout.forward(x_2) + x_1;

        if !self.norm_first {
            x_2 = cache
                .norm_2
                .forward_autoregressive(x_2, 1, |x_2| self.norm_2.forward(x_2));
        }

        x_2
    }
}

#[derive(Default)]
struct TransformerEncoderLayerAutoregressiveCache<B: Backend> {
    mha: MHAAutoregressiveCache<B>,
    pwff: TensorCache<B, 3>,
    norm_1: TensorCache<B, 3>,
    norm_2: TensorCache<B, 3>,
}

impl<B: Backend> TransformerEncoderLayerAutoregressiveCache<B> {
    fn new() -> Self {
        Self::default()
    }
}

/// Autoregressive cache for the [Transformer Encoder](TransformerEncoder) layer.
///
/// To be used during inference when decoding tokens.
pub struct TransformerEncoderAutoregressiveCache<B: Backend> {
    layers: Vec<TransformerEncoderLayerAutoregressiveCache<B>>,
}

impl<B: Backend> TransformerEncoderAutoregressiveCache<B> {
    fn empty(num_layers: usize) -> Self {
        Self {
            layers: (0..num_layers)
                .map(|_| TransformerEncoderLayerAutoregressiveCache::new())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{nn::attention::generate_autoregressive_mask, TestBackend};
    use burn_tensor::Distribution;

    #[test]
    fn test_autoregressive_norm_last() {
        let [d_model, d_ff, n_heads, num_layers] = [12, 24, 2, 3];
        test_autoregressive(
            TransformerEncoderConfig::new(d_model, d_ff, n_heads, num_layers)
                .with_norm_first(false),
        )
    }

    #[test]
    fn test_autoregressive_norm_first() {
        let [d_model, d_ff, n_heads, num_layers] = [12, 24, 2, 3];
        test_autoregressive(
            TransformerEncoderConfig::new(d_model, d_ff, n_heads, num_layers).with_norm_first(true),
        )
    }

    fn test_autoregressive(config: TransformerEncoderConfig) {
        let [batch_size, seq_length, d_model] = [3, 4, config.d_model];
        let transformer = TransformerEncoder::new(&config);

        let tensor = Tensor::<TestBackend, 3>::random(
            [batch_size, seq_length, d_model],
            Distribution::Standard,
        );
        let mask_attn = generate_autoregressive_mask(batch_size, seq_length, &tensor.device());
        let input = TransformerEncoderInput::new(tensor.clone()).mask_attn(mask_attn);

        let output_1 = transformer.forward(input);
        let mut output_2 = Vec::new();
        let mut cache = transformer.new_autoregressive_cache();

        for i in 1..seq_length + 1 {
            let tensor = tensor.clone().index([0..batch_size, 0..i, 0..d_model]);
            let input = TransformerEncoderInput::new(tensor.clone());
            let next_tok = transformer
                .forward_autoregressive_inference(input, &mut cache)
                .index([0..batch_size, i - 1..i, 0..d_model]);
            output_2.push(next_tok);
        }

        let output_2 = Tensor::cat(output_2, 1);

        output_1
            .into_data()
            .assert_approx_eq(&output_2.into_data(), 3);
    }
}
