#![cfg_attr(any(target_os = "solana", target_arch = "bpf"), no_std)]

use pinocchio::{
    AccountView, Address, ProgramResult, error::ProgramError, no_allocator, nostd_panic_handler,
    program_entrypoint,
};

mod constants;
mod instructions;
mod state;

use constants::*;
use instructions::*;
use state::*;

pub use winterwallet_common::ID;

program_entrypoint!(process_instruction);

nostd_panic_handler!();
no_allocator!();

fn process_instruction(
    _program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let (discriminator, instruction_data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    match discriminator {
        0 => Initialize::process(accounts, instruction_data),
        1 => Advance::process(accounts, instruction_data),
        2 => Withdraw::process(accounts, instruction_data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
