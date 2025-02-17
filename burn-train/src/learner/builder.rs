use super::log::update_log_file;
use super::Learner;
use crate::checkpoint::{AsyncCheckpointer, Checkpointer, FileCheckpointer};
use crate::logger::FileMetricLogger;
use crate::metric::dashboard::cli::CLIDashboardRenderer;
use crate::metric::dashboard::Dashboard;
use crate::metric::{Adaptor, Metric, Numeric};
use crate::AsyncTrainerCallback;
use burn_core::module::ADModule;
use burn_core::optim::Optimizer;
use burn_core::tensor::backend::ADBackend;
use burn_core::tensor::Element;
use std::sync::Arc;

/// Struct to configure and create a [learner](Learner).
pub struct LearnerBuilder<B, T, V>
where
    T: Send + Sync + 'static,
    V: Send + Sync + 'static,
    B: ADBackend,
{
    dashboard: Dashboard<T, V>,
    checkpointer_model: Option<Arc<dyn Checkpointer<B::FloatElem> + Send + Sync>>,
    checkpointer_optimizer: Option<Arc<dyn Checkpointer<B::FloatElem> + Send + Sync>>,
    num_epochs: usize,
    checkpoint: Option<usize>,
    directory: String,
    grad_accumulation: Option<usize>,
    devices: Vec<B::Device>,
}

impl<B, T, V> LearnerBuilder<B, T, V>
where
    T: Send + Sync + 'static,
    V: Send + Sync + 'static,
    B: ADBackend,
{
    pub fn new(directory: &str) -> Self {
        let renderer = Box::new(CLIDashboardRenderer::new());
        let logger_train = Box::new(FileMetricLogger::new(format!("{directory}/train").as_str()));
        let logger_valid = Box::new(FileMetricLogger::new(format!("{directory}/valid").as_str()));

        Self {
            dashboard: Dashboard::new(renderer, logger_train, logger_valid),
            num_epochs: 1,
            checkpoint: None,
            checkpointer_model: None,
            checkpointer_optimizer: None,
            directory: directory.to_string(),
            grad_accumulation: None,
            devices: vec![B::Device::default()],
        }
    }

    /// Register a training metric.
    pub fn metric_train<M: Metric + 'static>(mut self, metric: M) -> Self
    where
        T: Adaptor<M::Input>,
    {
        self.dashboard.register_train(metric);
        self
    }

    /// Register a validation metric.
    pub fn metric_valid<M: Metric + 'static>(mut self, metric: M) -> Self
    where
        V: Adaptor<M::Input>,
    {
        self.dashboard.register_valid(metric);
        self
    }

    /// Enable gradients accumulation.
    ///
    /// # Notes
    ///
    /// When you enable gradients accumulation, the gradients object used by the optimizer will be
    /// the sum of all gradients generated by each backward pass. It might be a good idea to
    /// reduce the learning to compensate.
    ///
    /// The effect is similar to increasing the `batch size` and the `learning rate` by the `accumulation`
    /// amount.
    pub fn grads_accumulation(mut self, accumulation: usize) -> Self {
        self.grad_accumulation = Some(accumulation);
        self
    }

    /// Register a training metric and displays it on a plot.
    ///
    /// # Notes
    ///
    /// Only [numeric](Numeric) metric can be displayed on a plot.
    /// If the same metric is also registered for the [validation split](Self::metric_valid_plot),
    /// the same graph will be used for both.
    pub fn metric_train_plot<M>(mut self, metric: M) -> Self
    where
        M: Metric + Numeric + 'static,
        T: Adaptor<M::Input>,
    {
        self.dashboard.register_train_plot(metric);
        self
    }

    /// Register a validation metric and displays it on a plot.
    ///
    /// # Notes
    ///
    /// Only [numeric](Numeric) metric can be displayed on a plot.
    /// If the same metric is also registered for the [training split](Self::metric_train_plot),
    /// the same graph will be used for both.
    pub fn metric_valid_plot<M: Metric + Numeric + 'static>(mut self, metric: M) -> Self
    where
        V: Adaptor<M::Input>,
    {
        self.dashboard.register_valid_plot(metric);
        self
    }

    /// The number of epochs the training should last.
    pub fn num_epochs(mut self, num_epochs: usize) -> Self {
        self.num_epochs = num_epochs;
        self
    }

    /// Run the training loop on multiple devices.
    pub fn devices(mut self, devices: Vec<B::Device>) -> Self {
        self.devices = devices;
        self
    }

    /// The epoch from which the training must resume.
    pub fn checkpoint(mut self, checkpoint: usize) -> Self {
        self.checkpoint = Some(checkpoint);
        self
    }

    /// Register a checkpointer that will save the [optimizer](crate::optim::Optimizer) and the
    /// [model](crate::module::Module) [states](crate::module::State).
    ///
    /// The number of checkpoints to be keep should be set to a minimum of two to be safe, since
    /// they are saved and deleted asynchronously and a crash during training might make a
    /// checkpoint non-usable.
    pub fn with_file_checkpointer<P: Element + serde::de::DeserializeOwned + serde::Serialize>(
        mut self,
        num_keep: usize,
    ) -> Self {
        self.checkpointer_model = Some(Arc::new(FileCheckpointer::<P>::new(
            format!("{}/checkpoint", self.directory).as_str(),
            "model",
            num_keep,
        )));
        self.checkpointer_optimizer = Some(Arc::new(FileCheckpointer::<P>::new(
            format!("{}/checkpoint", self.directory).as_str(),
            "optim",
            num_keep,
        )));
        self
    }

    /// Create the [learner](Learner) from a [module](ADModule) and an
    pub fn build<M, O>(self, model: M, optim: O) -> Learner<M, O, T, V>
    where
        M: ADModule<ADBackend = B>,
        O: Optimizer<Backend = B>,
    {
        self.init_logger();
        let callack = Box::new(self.dashboard);
        let callback = Box::new(AsyncTrainerCallback::new(callack));

        let create_checkpointer = |checkpointer| match checkpointer {
            Some(checkpointer) => {
                let checkpointer: Box<dyn Checkpointer<B::FloatElem>> =
                    Box::new(AsyncCheckpointer::new(checkpointer));
                Some(checkpointer)
            }
            None => None,
        };
        let model = model.detach();

        Learner {
            model,
            optim,
            num_epochs: self.num_epochs,
            callback,
            checkpoint: self.checkpoint,
            checkpointer_model: create_checkpointer(self.checkpointer_model),
            checkpointer_optimizer: create_checkpointer(self.checkpointer_optimizer),
            grad_accumulation: self.grad_accumulation,
            devices: self.devices,
        }
    }

    fn init_logger(&self) {
        let file_path = format!("{}/experiment.log", self.directory);
        update_log_file(file_path.as_str());
    }
}
