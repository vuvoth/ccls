use parser::event::Event;
use rowan::{GreenNodeBuilder, GreenNode};

/**
 * This crate receives events list in parser as input and return a tree - support API bellow: 
 * - Get element/node contain specific position
 * - Query to nodes contain the "same value" with receives node. 
 * - Reformat syntax tree
 * - Modify tree with metadata
 */


pub struct LosslessSyntaxTree;

// from string to GreenNode;
// impl LosslessSyntaxTree {
//     fn new(events: Vec<Event>) -> GreenNode {
//         let builder = GreenNodeBuilder::new();

        
//     }
// }