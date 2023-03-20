use std::marker::PhantomData;
use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::*,
    plonk::*,
    poly::Rotation,
    pasta::Fp, dev::MockProver,
};

// #[derive(Debug, Clone)] is a Rust attribute used to automatically generate implementations of the Debug and Clone traits for a struct
#[derive(Debug, Clone)]
struct ACell<F: FieldExt>(AssignedCell<F, F>);

#[derive(Debug, Clone)]
// This new version only has a single advice column
struct FiboConfig { 
    pub advice: Column<Advice>,
    pub selector: Selector,
    pub instance: Column<Instance>,
}

#[derive(Debug, Clone)]
struct FiboChip<F: FieldExt> {
    config: FiboConfig,
    _marker: PhantomData<F>,
}


impl<F: FieldExt> FiboChip<F> {

    pub fn construct(config: FiboConfig) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }

    // We modified it to take only one advice column
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advice: Column<Advice>,
        instance: Column<Instance>,  
    ) -> FiboConfig {
        // create the selector
        let selector: Selector = meta.selector();

        // We still need to enable the equality here. But we won't use it to perform the permutation checks as in the old version,
        // rather we will use it to perform the permutation check with the instance column in order to expose the public input
        meta.enable_equality(advice);
        meta.enable_equality(instance);

        // Now the copy constraint becomes a bit different! We have only one advise column and all the witness is passed to that advise column
        // a,b,c are all queriesd from the same advice column by performing some rotation. The custom gate has a different shape
        meta.create_gate("add", |meta| {
            // advice| selector
            // ----------------
            //  a    |    s
            //  b    |    
            //  c    |
            let s = meta.query_selector(selector);
            let a = meta.query_advice(advice, Rotation::cur());
            let b = meta.query_advice(advice, Rotation::next());
            let c = meta.query_advice(advice, Rotation(2));

            // This remains the same!
            vec![s * (a + b - c)] // s * (a + b - c) = 0
        }); 

        // return the configuration of the circuit. This included the advice columns and the selector, while the custom gates have been mutated on `meta`.
        FiboConfig { advice, selector, instance}
    }

    // The assignment is different now. We can no longer assign stuff row by row. If I were to assign values row by row, halo2 will be panicking
    // as I create a region that is not covering the whole custom gates. The solution is then the assign the entire table at the same time.
    // In this example we are gonna use a single region!
    fn assign(
        &self, 
        mut layouter: impl Layouter<F>, 
        nrows: usize,
    ) -> Result<AssignedCell<F, F>, Error> {
        layouter.assign_region(|| "entire fibonacci table", |mut region| {

            // We need to enable the selector in that region because the constraint is set!
            // The selector will be enable at each line as the copy constraint must be checked on each line!
            self.config.selector.enable(&mut region, 0)?;
            self.config.selector.enable(&mut region, 1)?;

            // this api is performing the assignment and the copy constaint from the instance column
            let mut a_cell = region.assign_advice_from_instance(|| "1", self.config.instance, 0, self.config.advice, 0)?;
            let mut b_cell = region.assign_advice_from_instance(|| "1", self.config.instance, 1, self.config.advice, 1)?;

            // we already assigned the first two rows, we need to assign all the other rows
            for row in 2..nrows {
                // The selector must enable to each row apart from the last 2 ones where we won't have any copy constraint!
                if row < nrows - 2 {
                    self.config.selector.enable(&mut region, row)?;
                }
                // compute value of c
                let c_val = a_cell.value().and_then(
                    |a| {
                        b_cell.value().map(|b| *a + *b)
                    }
                );

                // Assign c value to c cell. This will be the next row added to the table
                // important to note here that the offset inside the region is the row number 
                // 0 as offset inside the region mean the 0 row of the region!
                let c_cell = region.assign_advice(
                    || "advice",
                    self.config.advice,
                    row,
                    || c_val.ok_or(Error::Synthesis),
                )?;

                // Switch to the next step of the sequence
                a_cell = b_cell;
                b_cell = c_cell;
            }

            // We only need to return the last cell as we need to check if this matches the expected outputß 
            Ok(b_cell)
    })
}


    // create a function that takes an assigned cell and constrain that this must be the same as something inside the instance column
    // row is an absolute row number inside the instance column against which to perform this equality check
    pub fn expose_public(&self, mut layouter: impl Layouter<F>, cell: AssignedCell<F, F>, row:usize) -> Result<(), Error> {
        // Given an assigned cell and a row number inside the given instance column enforce equality
        // In this case there's only 1 config instance. I access it from the self.config method
        layouter.constrain_instance(cell.cell(), self.config.instance, row)
    }
}

#[derive(Default)]

// We define the circuit with the field a, b which are the input values for our circuit
struct MyCircuit<F>(PhantomData<F>);

impl<F: FieldExt> Circuit<F> for MyCircuit<F> {
    type Config = FiboConfig;
    type FloorPlanner = SimpleFloorPlanner;

    // It generates an empty circuit without any witness
    // You can use this api to generate proving key or verification key without any witness
    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    // create configuration for the Circuit
    fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
        let advice = meta.advice_column();
        let instance = meta.instance_column();
        FiboChip::configure(meta, advice, instance)
    } 
    
    fn synthesize(&self, config: Self::Config, mut layouter: impl Layouter<F>) -> Result<(), Error> {
        // We create a new instance of chip using the config passed as input
        let chip = FiboChip::construct(config);

        // We no longer need these functions as the copy constraint is already enforced by assign_advice_from_instance function.
        // chip.expose_public(layouter.namespace(|| "private a"), &prev_a, 0);
        // chip.expose_public(layouter.namespace(|| "private b"), &prev_b, 1);

        // First we assign the rows
        let out_cell = chip.assign(
            layouter.namespace(|| "entire table"),
            10
        )?;

        // Check that the last cell matches the output. Here we need to enforce the copy constraint!
        chip.expose_public(layouter.namespace(|| "output"), out_cell, 2)?;

        Ok(())
    }

}

fn main() { 
    let k = 4;
    let a = Fp::from(1);
    let b = Fp::from(1);
    let out = Fp::from(55);

    // We no longer need to pass a,b inside the circuit struct as these are already specified in the instance column
    // It would make sense to keep the value here only if these were passed to the circuit as private input
    let circuit = MyCircuit(PhantomData);

    let public_input = vec![a, b, out];

    // The mock prover is a function that execute the configuration of the circuit by running its method configure
    // and also execute the syntetize function, by passing in the actual input.
    // The instance vector is filled by the values that will be used inside the instance column
    let prover = MockProver::run(k, &circuit, vec![public_input.clone()]).unwrap();

    prover.assert_satisfied();

    print_circuit();

}

#[cfg(feature = "dev-graph")]
fn print_circuit() {
        use plotters::prelude::*;
        let root = BitMapBackend::new("fib-3-layout.png", (1024, 3096)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root.titled("Fib 3 Layout", ("sans-serif", 60)).unwrap();

        let circuit = MyCircuit::<Fp>(PhantomData);
        halo2_proofs::dev::CircuitLayout::default()
            .render(4, &circuit, &root)
            .unwrap();
}

