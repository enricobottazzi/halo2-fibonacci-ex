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

// If you look back into the circuit description we have 3 advice columns
// 1 selector column and 1 instance colums. This is a column that encodes the public input!
struct FiboConfig { 
    pub advice: [ Column<Advice>; 3],
    pub selector: Selector,
    pub instance: Column<Instance>,
}

// struct that is bounded to a generic type <F:FieldExt>
struct FiboChip<F: FieldExt>  {
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
        advice: [Column<Advice>; 3],
        instance: Column<Instance>,  
    ) -> FiboConfig {
        // create the 3 advice colums
        let col_a: Column<Advice> = advice[0];
        let col_b: Column<Advice> = advice[1];
        let col_c: Column<Advice> = advice[2];
        // create the selector
        let selector: Selector = meta.selector();

        // In order to perform the permutation check later on we need to enable equality
        // By enabling equality, we tell the halo2 compiler that these columns are gonna be used inside the permutation check.
        // If we don't enable it, we won't be able to perform the permutation check.
        meta.enable_equality(col_a);
        meta.enable_equality(col_b);
        meta.enable_equality(col_c);
        // We also enable equality on the instance column as we need to execute a permutation check on that too!
        meta.enable_equality(instance);

        // create custom gate. This is the first constraint (of custom gate type) described in fibonacci-constraint-1.png
        meta.create_gate("add", |meta| {
            // col_a | col_b | col_c | selector
            // ---------------------------------
            //  a    |   b   |   c   |   s
            // We are now querying 4 cells from a single row. The selector has no rotation as it is like coordinating the whole thing.
            // For the advice colums, we are querying the current row as the rotation is set to cur.
            // What you are querying in the advice columns is relative to the selector. If the selector is turned on
            // then the advice column's cells are enabled to be used. If we turn the selector on, the advice columns will be used for this row
            // If we use the rotation next for column c we'll be querying the value inside the instance column for the next row. 
            // In this way we can define a different shape for our custom gate. 
            // col_a | col_b | col_c | selector
            // ---------------------------------
            //  a    |   b   |       |   s
            //       |       |   c   |   
            // Query returns you a cell from a column to be used inside the custom gate j
            let s = meta.query_selector(selector);
            let a = meta.query_advice(col_a, Rotation::cur());
            let b = meta.query_advice(col_b, Rotation::cur());
            let c = meta.query_advice(col_c, Rotation::cur());

            // return the contraint(s) inside our custom gate. You can define as many
            // constraints as you want inside the same custom gate
            // If selector is turned off, the constraint will be satisfied whatever value is assigned to a,b,c 
            vec![s * (a + b - c)] // s * (a + b - c) = 0
        }); 

        // return the configuration of the circuit. This included the advice columns and the selector, while the custom gates have been mutated on `meta`.
        FiboConfig { advice: [col_a, col_b, col_c ], selector, instance}
    }

    // This is the function used inside syntentize to assign value to the first raw of the circuit.
    // mut layouter: impl Layouter<F> specifies a function parameter named layouter, which is mutable (mut keyword), and implements the Layouter<F> trait.
    // a and b value will be provided to this function as input. This are the a and b to be assigned inside the first row.
    fn assign_first_row(&self, mut layouter: impl Layouter<F>, a: Option<F>, b: Option<F>) -> Result<(ACell<F>, ACell<F>, ACell<F>), Error> {
        layouter.assign_region(|| "first row", |mut region| {

            // We need to enable the selector in that region because the constraint is set!
            self.config.selector.enable(&mut region, 0);

            // Assign the value to a and b. It returns an assigned cell!
            let a_cell = region.assign_advice(
                || "a", // we are assigning to column a
                self.config.advice[0], 
                0, 
                || a.ok_or(Error::Synthesis),
             ).map(ACell)?;

             let b_cell = region.assign_advice(
                || "b",
                self.config.advice[1], 
                0, 
                || b.ok_or(Error::Synthesis),
             ).map(ACell)?;

             // Then we compute the value c and later assign it to c_cell. C=a+b
             let c_val = a.and_then(|a| b.map(|b| a+b));

             let c_cell = region.assign_advice(
                || "c",
                self.config.advice[2], 
                0, 
                || c_val.ok_or(Error::Synthesis),
             ).map(ACell)?;

            Ok((a_cell, b_cell, c_cell))
    })
}

    // This function takes a layouter in and cells from the previous row and assign value for the current row.
    fn assign_row(&self, mut layouter: impl Layouter<F>, prev_b: &ACell<F>, prev_c: &ACell<F>)  -> Result<ACell<F>, Error> {
        
        // Create permutation check contraints. This is the first constraint (of permutation type) described in fibonacci-constraint-2.png
        layouter.assign_region(
            || "next row",
            |mut region| {
                // Here we turn on the selector gate
                self.config.selector.enable(&mut region, 0);
                // In this line I'm trying to copy stuff from the previous row to the new region in the current row
                // This is the copy constraint basically
                // I'm copying the prev_b to the current region in advice column 0 (aka "a")
                // Offset 0 means that I'm copying to the first row in the region
                // copy advice is the permutation check!
                prev_b.0.copy_advice(|| "a", &mut region, self.config.advice[0], 0)?;
                // I'm copying the prev_c to the current region in advice column 1 (aka "b")
                // Offset 0 means that I'm copying to the first row in the region
                prev_c.0.copy_advice(|| "b", &mut region, self.config.advice[1], 0)?;

                // Lastly, we access the values from prev_b and prev_c and add them together to get the c_val 
                let c_val = prev_b.0.value().and_then(
                    |b| {
                        prev_c.0.value().map(|c| *b + *c)
                    }
                );

                // We create the c_cell for the current row by assign the c_val to it!
                let c_cell = region.assign_advice(
                    || "c",
                    self.config.advice[2],
                    0,
                    || c_val.ok_or(Error::Synthesis),
                ).map(ACell)?;

                Ok(c_cell)
            })
    }

    // create a function that takes an assigned cell and constrain that this must be the same as something inside the instance column
    // row is an absolute row number inside the instance column against which to perform this equality check
    pub fn expose_public(&self, mut layouter: impl Layouter<F>, cell: &ACell<F>, row:usize) -> Result<(), Error> {
        // Given an assigned cell and a row number inside the given instance column enforce equality
        // In this case there's only 1 config instance. I access it from the self.config method
        layouter.constrain_instance(cell.0.cell(), self.config.instance, row)
    }
}

#[derive(Default)]

// We define the circuit with the field a, b which are the input values for our circuit
struct MyCircuit<F> {
    pub a: Option<F>,
    pub b: Option<F>,
}

impl<F: FieldExt> Circuit<F> for MyCircuit<F> {
    type Config = FiboConfig;
    type FloorPlanner = SimpleFloorPlanner;

    // It generates an empty circuit without any witness
    // You can use this api to generate proving key or verification key without any witness
    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    // create configuration for the Circuit
    // We create the columns here such that these can be passed trough different chip configuration
    fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
        let col_a = meta.advice_column();
        let col_b = meta.advice_column();
        let col_c = meta.advice_column();

        let instance = meta.instance_column();

        FiboChip::configure(meta, [col_a, col_b, col_c], instance)
    } 
    
    // API to be called after the constraint system is defined.
    // Assign the values inside the actual prover input inside the circuit.
    // mut layouter: impl Layouter<F> specifies a function parameter named layouter, which is mutable (mut keyword), and implements the Layouter<F> trait.
    fn synthesize(&self, config: Self::Config, mut layouter: impl Layouter<F>) -> Result<(), Error> {
        // We create a new instance of chip using the config passed as input
        let chip = FiboChip::construct(config);
        // now we assign stuff inside the circuit!
        // first row is particular so we create a specific function for that.
        // This function will take as input the "a" and "b" value passed to instantiate the circuit
        // We also use a layouter as this is a good way to separate different regions of the circuit
        // We can also assign name to the layouter
        let (mut prev_a, mut prev_b, mut prev_c) = chip.assign_first_row(layouter.namespace(|| "first row"), self.a, self.b)?;

        // Let's now use the expose_public function to enforce some assigned value to be equal to the ones described inside the instance column
        // In the row 0 of the instance column (public values) we are gonna pass the value that we want prev_a to be constrained to be equal to
        chip.expose_public(layouter.namespace(|| "private a"), &prev_a, 0);
        // same to b
        chip.expose_public(layouter.namespace(|| "private b"), &prev_b, 1);

        // Now we have assigned the first row! Now we have to assign the other rows! Remember that the idea of the circuit was
        // given f(0) = x, f(1) = y, we will prove f(9) = z. We already have assigned f(0) and f(1). We now need to assign values to the other rows. 
        for _i in 3..10 {
            let c_cell  = chip.assign_row(
                layouter.namespace(|| "next row"),
                &prev_b,
                &prev_c,
            )?;

            prev_b = prev_c;
            prev_c = c_cell;
        }

        // Also we want to expose the output of the circuit to the public
        chip.expose_public(layouter.namespace(|| "output"), &prev_c, 2);

        Ok(())
    }

}

fn main() { 
    let k = 4;
    let a = Fp::from(1);
    let b = Fp::from(1);
    let out = Fp::from(55);

    let circuit = MyCircuit {
        a: Some(a),
        b: Some(b),
    };

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
    let root = BitMapBackend::new("fib-2-layout.png", (1024, 3096)).into_drawing_area();
    root.fill(&WHITE).unwrap();
    let root = root.titled("Fib 2 Layout", ("sans-serif", 60)).unwrap();

    let a = Fp::from(1);
    let b = Fp::from(1);

    let circuit = MyCircuit {
        a: Some(a),
        b: Some(b),
    };
    
    halo2_proofs::dev::CircuitLayout::default()
        .render(4, &circuit, &root)
        .unwrap();
}