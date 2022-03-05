// import * as anchor from "@project-serum/anchor";
// import { Program } from "@project-serum/anchor";
// import { Myepicproject } from "../target/types/myepicproject";

const anchor = require("@project-serum/anchor");
const { SystemProgram } = require("@solana/web3.js");

const main = async () => {
  console.log("Starting test...");

  // Create and set the provider. We set it before but we needed to update it, so that it can commnicate with our frontend!
  const provider = anchor.Provider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Myepicproject;

  // Create an account keypair for our program to use.
  const baseAccount = anchor.web3.Keypair.generate();

  // Call start_stuff_off, pass it the params it needs!
  let tx = await program.rpc.startStuffOff({
    accounts: {
      baseAccount: baseAccount.publicKey,
      user: provider.wallet.publicKey,
      systemProgram: SystemProgram.programId,
    },
    signers: [baseAccount],
  })
  console.log("Your transaction signature", tx);

  // const tx = await program.rpc.startStuffOff();
  // Fetch data from the account.
  let account = await program.account.baseAccount.fetch(baseAccount.publicKey);
  console.log("Gif Count", account.totalGifs.toString());

  // Call add_gifs!
  // You will need to now pass a GIF link to the function! You will need to pass in the user submitting the GIF!
  await program.rpc.addGif("https://i.giphy.com/media/eIG0HfouRQJQr1wBzz/giphy.webp", {
    accounts: {
      baseAccount: baseAccount.publicKey,
      user: provider.wallet.publicKey,
    },
  });

  account = await program.account.baseAccount.fetch(baseAccount.publicKey);
  console.log("Gif Count", account.totalGifs.toString());

  // Access gif_list on the account!
  console.log("GIF Count", account.gifList);
};

const runMain = async () => {
  try {
    await main();
    process.exit(0);
  } catch (error) {
    console.error(error);
    process.exit(1);
  }
};

runMain();

// describe("myepicproject", () => {
//   // Configure the client to use the local cluster.
//   anchor.setProvider(anchor.Provider.env());

//   const program = anchor.workspace.Myepicproject as Program<Myepicproject>;

//   it("Is initialized!", async () => {
//     // Add your test here.
//     const tx = await program.rpc.initialize({});
//     console.log("Your transaction signature", tx);
//   });
// });
