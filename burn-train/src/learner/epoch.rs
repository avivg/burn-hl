use burn_core::{
    data::dataloader::DataLoader,
    module::ADModule,
    optim::{GradientsAccumulator, Optimizer},
    tensor::backend::Backend,
};
use std::sync::Arc;

use crate::{LearnerCallback, LearnerItem, MultiDevicesTrainStep, TrainStep, ValidStep};

#[derive(new)]
pub struct ValidEpoch<VI> {
    dataloader: Arc<dyn DataLoader<VI>>,
    epoch: usize,
    epoch_total: usize,
}

#[derive(new)]
pub struct TrainEpoch<TI> {
    dataloader: Arc<dyn DataLoader<TI>>,
    epoch: usize,
    epoch_total: usize,
    grad_accumulation: Option<usize>,
}

impl<I> ValidEpoch<I> {
    pub fn run<M, TO, VO>(&self, model: M, callback: &mut Box<dyn LearnerCallback<TO, VO>>) -> M
    where
        M: ADModule,
        M::InnerModule: ValidStep<I, VO>,
    {
        log::info!("Executing validation step for epoch {}", self.epoch);
        let model = model.inner();

        let mut iterator = self.dataloader.iter();
        let mut iteration = 0;

        while let Some(item) = iterator.next() {
            let progress = iterator.progress();
            iteration += 1;

            let item = model.step(item);
            callback.on_valid_item(LearnerItem::new(
                item,
                progress,
                self.epoch,
                self.epoch_total,
                iteration,
            ));
        }
        callback.on_valid_end_epoch(self.epoch);

        ADModule::from_inner(model)
    }
}

impl<TI> TrainEpoch<TI> {
    pub fn run<M, O, TO, VO>(
        &self,
        mut model: M,
        mut optim: O,
        callback: &mut Box<dyn LearnerCallback<TO, VO>>,
    ) -> (M, O)
    where
        M: ADModule,
        O: Optimizer<Backend = M::ADBackend>,
        M: TrainStep<TI, TO>,
    {
        log::info!("Executing training step for epoch {}", self.epoch,);

        let mut iterator = self.dataloader.iter();
        let mut iteration = 0;
        let mut accumulator = GradientsAccumulator::new();
        let mut accumulation_current = 0;

        while let Some(item) = iterator.next() {
            iteration += 1;

            let progress = iterator.progress();
            let item = model.step(item);

            match self.grad_accumulation {
                Some(accumulation) => {
                    accumulator.accumulate(&model, item.grads);
                    accumulation_current += 1;

                    if accumulation <= accumulation_current {
                        let grads = accumulator.grads();
                        model = optim.update_module(model, grads);
                        accumulation_current = 0;
                    }
                }
                None => model = optim.update_module(model, item.grads),
            }

            callback.on_train_item(LearnerItem::new(
                item.item,
                progress,
                self.epoch,
                self.epoch_total,
                iteration,
            ));
        }
        callback.on_train_end_epoch(self.epoch);

        (model, optim)
    }
}

impl<TI> TrainEpoch<TI> {
    pub fn run_multi_device<M, O, TO, VO>(
        &self,
        mut model: M,
        mut optim: O,
        callback: &mut Box<dyn LearnerCallback<TO, VO>>,
        devices: Vec<<M::Backend as Backend>::Device>,
    ) -> (M, O)
    where
        O: Optimizer<Backend = M::ADBackend>,
        M: TrainStep<TI, TO>,
        M: ADModule + 'static,
        TI: Send + 'static,
        TO: Send + 'static,
    {
        log::info!(
            "Executing training step for epoch {} on devices {:?}",
            self.epoch,
            devices
        );

        let mut iterator = self.dataloader.iter();
        let mut iteration = 0;
        let mut accumulator = GradientsAccumulator::new();
        let mut accumulation_current = 0;

        let accumulation = self.grad_accumulation.unwrap_or(1) * devices.len();
        let step = MultiDevicesTrainStep::new(&devices);

        // The main device is always the first in the list.
        let device_main = devices.get(0).unwrap().clone();

        loop {
            let items = step.step(&mut iterator, &model);
            if items.is_empty() {
                break;
            }

            for item in items {
                iteration += 1;
                let progress = iterator.progress();

                let grads = item.grads.to_device(&device_main, &model);

                log::info!("Updated device");
                accumulator.accumulate(&model, grads);
                accumulation_current += 1;

                if accumulation <= accumulation_current {
                    let grads = accumulator.grads();
                    model = optim.update_module(model, grads);
                    accumulation_current = 0;
                }

                callback.on_train_item(LearnerItem::new(
                    item.item,
                    progress,
                    self.epoch,
                    self.epoch_total,
                    iteration,
                ));
            }
        }

        callback.on_train_end_epoch(self.epoch);

        (model, optim)
    }
}
