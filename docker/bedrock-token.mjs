import { getTokenProvider } from "@aws/bedrock-token-generator";

const token = await getTokenProvider()();
process.stdout.write(token);
