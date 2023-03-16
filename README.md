- Do Fixed colums need to be at the same size of the others columns?
- Are you using only one columns set inside a halo2 circuit or you can compose different ones?
- What are the layouters?
- Do you need to supply the entire witness or can you let the circuit generate it by itself?
- Is there a solidity snark verifier for halo2?
- Is the name of the constraint something arbitrary or is this encoded in the package? A: Yes we can set any name we want
- What is the difference between the witness and the public input?
- What is the rotation doing? You can also use the Rotation::next() method to get the next row rather than cur().
`let c = meta.query_advice(col_c, Rotation::next())`

A: This has been explained inside example1.rs

You can also specify the off-set you want to query by using the Rotation(0) method.

- When would you even set the selector to 0? 

You can set the selector to 0 when you want to skip a constraint. Also the number of rows is always 2^k, so if you are not using the entire rows you can set the selector to 0.

- What is derive doing in Rust??
- What is FloorPlanner? I didn't understand it..
- What is the offset?
- Where does region come from in assign_region method?
- Is `copy_advice` the way to set copy constraints? 
- K is the size of the circuit. What does that mean?
- What does pub a and pub b mean? Are these set as public input to the circuit?
- What should we pass inside instance? Why do we pass an empty vector?
- What is a fiboChip, how do you define it?
- Where am I enforcing the permutation check?
- What does meta mean?
- What is offset and what is the layouter, region?




# Fibonacci Circuit 

The goal is that given f(0)=x and f(1)=y, we will prove that f(9)=z

### Example 1

<img src="./img/fibonacci-table-1.png"  width="60%" height="30%">

- Go into cargo.toml and add dependencies to that


Run 

```cargo run --bin example1```

## General structure 

**FiboChip** : Create fiboChip, 

**Config**: need to set the config