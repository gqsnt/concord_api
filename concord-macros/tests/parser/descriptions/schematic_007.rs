pub client S3(region: Region, signer: Signer) { transport SigV4::new(signer); … }
