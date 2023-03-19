use std::marker::PhantomData;
use plotters::prelude::*;
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
// If you look back into the circuit description we have 3 advice columns
// 1 selector column and 1 instance colums. We can ignore the instance column for now. This is a column that encodes the public input!
struct FiboConfig { 
    pub advice: Column<Advice>,
    pub selector: Selector,
    pub instance: Column<Instance>,
}

#[derive(Debug, Clone)]
// struct that is bounded to a generic type <F:FieldExt>
struct FiboChip<F: FieldExt> {
    config: FiboConfig,
    _marker: PhantomData<F>,
}


// Now we add methods to this FiboChip struct. Impl is a keyword that let us add methods to a struct.
// impl<F: FieldExt> FiboChip<F> defines an implementation of the FiboChip struct for a generic type parameter F that implements the FieldExt trait
impl<F: FieldExt> FiboChip<F> {

    // This method is the constructor for the chip!
    pub fn construct(config: FiboConfig) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }

    // This method is where we define the Config of the chip by creating colums 
    // and defining custom gates
    // In this example we also pass the advice and instance colums directly to the configure function. by doing that we can create 
    // columns that can be shared across different configs.
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advice: Column<Advice>,
        instance: Column<Instance>,  
    ) -> FiboConfig {
        // create the selector
        let selector: Selector = meta.selector();

        // In order to perform the permutation check later on we need to enable equality
        // By enabling equality, we tell the halo2 compiler that these columns are gonna be used inside the permutation check.
        // If we don't enable it, we won't be able to perform the permutation check.
        meta.enable_equality(advice);
        meta.enable_equality(instance);

        // Now the copy constraint becomes a bit different! We have only one advise column and all the witness is passed to that advise column
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

            // return the contraint(s) inside our custom gate. You can define as many
            // constraints as you want inside the same custom gate
            // If selector is turned off, the constraint will be satisfied whatever value is assigned to a,b,c 
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
            self.config.selector.enable(&mut region, 0)?;
            self.config.selector.enable(&mut region, 1)?;

            let mut a_cell = region.assign_advice_from_instance(|| "1", self.config.instance, 0, self.config.advice, 0)?;
            let mut b_cell = region.assign_advice_from_instance(|| "1", self.config.instance, 1, self.config.advice, 1)?;

            for row in 2..nrows {
                if row < nrows - 2 {
                    self.config.selector.enable(&mut region, row)?;
                }
                // retrieve value of c
                let c_val = a_cell.value().and_then(
                    |a| {
                        b_cell.value().map(|b| *a + *b)
                    }
                );

                // Assign value to c cell
                let c_cell = region.assign_advice(
                    || "advice",
                    self.config.advice,
                    row,
                    || c_val.ok_or(Error::Synthesis),
                )?;

                a_cell = b_cell;
                b_cell = c_cell;
            }

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
    
    // API to be called after the constraint system is defined.
    // Assign the values inside the actual prover input inside the circuit.
    // mut layouter: impl Layouter<F> specifies a function parameter named layouter, which is mutable (mut keyword), and implements the Layouter<F> trait.
    fn synthesize(&self, config: Self::Config, mut layouter: impl Layouter<F>) -> Result<(), Error> {
        // We create a new instance of chip using the config passed as input
        let chip = FiboChip::construct(config);

        let out_cell = chip.assign(
            layouter.namespace(|| "entire table"), 
            10
        )?;

        // now we assign stuff inside the circuit!
        // first row is particular so we create a specific function for that.
        // This function will take as input the "a" and "b" value passed to instantiate the circuit
        // We also use a layouter as this is a good way to separate different regions of the circuit
        // We can also assign name to the layouter 
        // Also we want to expose the output of the circuit to the public
        chip.expose_public(layouter.namespace(|| "output"), out_cell, 2)?;

        Ok(())
    }

}

fn main() { 
    let k = 4;
    let a = Fp::from(1);
    let b = Fp::from(1);
    let out = Fp::from(55);

    let circuit = MyCircuit(PhantomData);

    let public_input = vec![a, b, out];

    // The mock prover is a function that execute the configuration of the circuit by running its method configure
    // and also execute the syntetize function, by passing in the actual input.
    // The instance vector is filled by the values that will be used inside the instance column
    let prover = MockProver::run(k, &circuit, vec![public_input.clone()]).unwrap();

    prover.assert_satisfied();

}

fn print_circuit() {
        // Create the area you want to draw on.
    // Use SVGBackend if you want to render to .svg instead.
    let root = BitMapBackend::new("layout.png", (1024, 768)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let root = root
        .titled("Example Circuit Layout", ("sans-serif", 60))
        .unwrap();

    let circuit = MyCircuit::<Fp>(PhantomData);

    halo2_proofs::dev::CircuitLayout::default()
        // You can optionally render only a section of the circuit.
        .view_width(0..2)
        .view_height(0..16)
        // You can hide labels, which can be useful with smaller areas.
        .show_labels(false)
        // Render the circuit onto your area!
        // The first argument is the size parameter for the circuit.
        .render(5, &circuit, &root)
        .unwrap();

}


 


