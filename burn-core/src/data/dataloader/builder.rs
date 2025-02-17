use super::{batcher::Batcher, BatchDataLoader, BatchStrategy, DataLoader, FixBatchStrategy};
use burn_dataset::{transform::ShuffledDataset, Dataset};
use std::sync::Arc;

pub struct DataLoaderBuilder<I, O> {
    strategy: Option<Box<dyn BatchStrategy<I>>>,
    batcher: Arc<dyn Batcher<I, O>>,
    num_threads: Option<usize>,
    shuffle: Option<u64>,
}

impl<I, O> DataLoaderBuilder<I, O>
where
    I: Send + Sync + Clone + std::fmt::Debug + 'static,
    O: Send + Sync + Clone + std::fmt::Debug + 'static,
{
    pub fn new(batcher: Arc<dyn Batcher<I, O>>) -> Self {
        Self {
            batcher,
            strategy: None,
            num_threads: None,
            shuffle: None,
        }
    }

    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.strategy = Some(Box::new(FixBatchStrategy::new(batch_size)));
        self
    }

    pub fn shuffle(mut self, seed: u64) -> Self {
        self.shuffle = Some(seed);
        self
    }

    pub fn num_workers(mut self, num_workers: usize) -> Self {
        self.num_threads = Some(num_workers);
        self
    }

    pub fn build(self, dataset: Arc<dyn Dataset<I>>) -> Arc<dyn DataLoader<O>> {
        let dataset = match self.shuffle {
            Some(seed) => Arc::new(ShuffledDataset::with_seed(dataset, seed)),
            None => dataset,
        };
        let strategy = match self.strategy {
            Some(strategy) => strategy,
            None => Box::new(FixBatchStrategy::new(1)),
        };
        if let Some(num_threads) = self.num_threads {
            return Arc::new(BatchDataLoader::multi_thread(
                strategy,
                dataset,
                self.batcher,
                num_threads,
            ));
        }

        Arc::new(BatchDataLoader::new(strategy, dataset, self.batcher))
    }
}
