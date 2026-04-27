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
    let (disc, instruction_data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;
    match *disc {
        discriminator::INITIALIZE => Initialize::process(accounts, instruction_data),
        discriminator::ADVANCE => Advance::process(accounts, instruction_data),
        discriminator::WITHDRAW => Withdraw::process(accounts, instruction_data),
        discriminator::CLOSE => Close::process(accounts, instruction_data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
