// The commits of the extracted data repository (MC_EXTRACTED_DATA_REPO) that hold a release's
// Mojang-named client sources, for the releases with no local tree of those names. Each embeds the
// release it is pinned for (scripts/materialize-sources.cjs refuses it otherwise), so a branch name
// is never the evidence: origin/client1.16.4 embeds 20w45a and is not here. 26.2 and 26.3 are read
// from MC_SOURCE_ROOT.
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");

const PINNED_SOURCES = {
  "1.14.4": "16d77f2e0928e7c2c452e4c7917ab32b199f0034",
  "1.18.1": "b558d004f45790b779d46f99a48da8230f8ed305",
  "1.19": "2bcb10e888eeee163636b9292d8e906b9ee98d4e",
  "1.19.1": "38b266a376a7c3a1f6466359f8a18926fbb89acf",
  "1.19.3": "4b469221a8a4559e634823d931dcae26d59f548e",
  "1.19.4": "20e005fafc4928179f144d3ff68413f0ca70808b",
  "1.20": "aaa632b95f0d0f45b75af0e820807771fdfe3711",
  "1.20.1": "d4cef5e3a820a44e909f9bf2774ac47a6d345279",
  "1.20.2": "31536fd92d1808232f8c92ed4e2cc8efe648afa9",
  "1.20.3": "ff36e37f7f228d6c5ab53ad7632c39edc39fe50f",
  "1.20.4": "6cc5ed552c39085fc7243a2ae0dcd96344f87620",
  "1.20.5": "b5eb2183e5c7cfd016c59a61ac97dd31c76f89e6",
  "1.20.6": "705aa0bfd647f073bfe01b54f7481b5e73c0eb9f",
  "1.21": "c7b74b7fff4b617bc2d579fb2a8e7b2912061c25",
  "1.21.1": "ffa803ec96617e6d6dd820d974608f4ad866c8ae",
  "1.21.2": "5039f20f44683c70125a85e62880c2e78798228e",
  "1.21.3": "c3b80bfb89ae12572a77595271672bcbdb3c390b",
  "1.21.4": "43a29ca80bb329f75b92695317a522492e25c2e7",
  "1.21.5": "860a1f0798a1e66f0d3bf4d898940437635f1ef8",
  "1.21.6": "72f4898ae9886323e7b78e639b8ceaceb4e20df7",
  "1.21.7": "e86fb0504027e61e66ff8ff9bbe9b0b178f0a85f",
  "1.21.8": "fa8d87bf19ddbf9f1f266dd4d47954a9743739fb",
  "1.21.9": "d994b26189ca5fcb3b614a935714cffb1dc0863e",
  "1.21.10": "bd23da1145bce3e596ff2592d8b07e248255ce7e",
  "1.21.11": "09859053de9898f726a798244c1d219ec1d42dee",
  "26.1": "250494672ff7a6ef66607c785c91a56a5c32c9f3",
  "26.1.1": "2322018b5712b13eacbbb324b7f9dd359091cdc7",
};

// The releases read from MC_SOURCE_ROOT (<release>/client), and the commits of the extracted data repository that
// scripts/materialize-sources.cjs unpacks there: origin/client26.2 and origin/client26.3, whose files are those of the
// E:/pr/mc-source trees the generated data was made from, line endings aside.
const PINNED_ROOT_SOURCES = {
  "26.2": "912ae031240da395051acae238a3f60d733353de",
  "26.3": "7c0695f918f2b96b09b66169ba54b7aa6e85244f",
};

// The releases no commit holds: their sources are decompiled from Mojang's own client
// (scripts/materialize-sources.cjs). `client` is the sha1 Mojang's version json gives the client jar.
// 26.x ships unobfuscated, so the client jar is the input. Before, the input is that jar remapped to
// Mojang's names by unimined, pinned by its sha256 (`remapped`) with the sha1 of the mappings in
// generated/mojang-releases.json checked beside it.
const PINNED_CLIENTS = {
  "1.15": { client: "7b07fd09d1e3aae1bc7a1304fedc73bfe5d81800", remapped: "aac72c6d121c7b4890f494df91c7b859a4ef623f37f34a4d0331250df67eaff0" },
  "1.15.1": { client: "8b11614bea9293592a947ea8f4fd72981ea66677", remapped: "9ad2444c74f090833e09641f396dfc14bcaa2ad2c84d675eb22802010e29bb11" },
  "1.15.2": { client: "e3f78cd16f9eb9a52307ed96ebec64241cc5b32d", remapped: "474f05aa32df28632e3314287d5b4c7ec1d675e233749d1ed38ee350fc64e6f8" },
  "1.16": { client: "228fdf45541c4c2fe8aec4f20e880cb8fcd46621", remapped: "521a21a07e669542a7a626521f709b581a0f9014b3dad2f80da9a93d59a428b3" },
  "1.16.1": { client: "c9abbe8ee4fa490751ca70635340b7cf00db83ff", remapped: "452f7aa006fae45df63e99363aa01cf98cd62172f04b7b1017581e3b1afa0168" },
  "1.16.2": { client: "653e97a2d1d76f87653f02242d243cdee48a5144", remapped: "1a962a763432a27484c6571f0f10eb98f1bfc124ece0c49f787a593e74954a72" },
  "1.16.3": { client: "1321521b2caf934f7fc9665aab7e059a7b2bfcdf", remapped: "b6a70751506db5ace1908982642eb57953f7b596151d5889c468e4333d55928e" },
  "1.16.4": { client: "1952d94a0784e7abda230aae6a1e8fc0522dba99", remapped: "0a1f709402db0ad07e554b99a6bb0ee58b01e454b1a7f10cac3e24c1727acfed" },
  "1.16.5": { client: "37fd3c903861eeff3bc24b71eed48f828b5269c8", remapped: "5ed9b54f988ba6021a0acdeb8b99d7f097300cc033b2bf30a87d5f1858f174ab" },
  "1.17": { client: "1cf89c77ed5e72401b869f66410934804f3d6f52", remapped: "553ac7daf1185b3597861b3653447c1aff86e39b18ffdd01a20cd37d71dbfd6d" },
  "1.17.1": { client: "8d9b65467c7913fcf6f5b2e729d44a1e00fde150", remapped: "8df36660ba33f406ceafdc9d36f101c28a2bf123332fee425d745a9e1f2b3490" },
  "1.18": { client: "d49eb6caed53d23927648c97451503442f9e26fd", remapped: "c95a274d63375a228d1cf689242b3abf87a2d3c452d46e48472f9134927dfc74" },
  "1.18.2": { client: "2e9a3e3107cca00d6bc9c97bf7d149cae163ef21", remapped: "cdc4f7eab0026b88ef5bf15a454826afc7d5bd342c0a84e9eee99f7d43b13ad7" },
  "1.19.2": { client: "055b30d860ead928cba3849ba920c88b6950b654", remapped: "38d622befdc24edc77c2859bbaad549d2ce5717f8a33a77c20b0624a423a745b" },
  "26.1.2": { client: "4e618f09a0c649dde3fdf829df443ce0b8831e65" },
};

// The clients whose sources come from elsewhere, for what their sources lack: the default data pack,
// whose tags are the release's exact memberships. The sha1 is the one Mojang's version json gives.
const CLIENT_DATA = {
  "1.14.4": "8c325a0c5bd674dd747d6ebaa4c791fd363ad8a9",
  "1.18.1": "7e46fb47609401970e2818989fa584fd467cd036",
  "1.19": "c0898ec7c6a5a2eaa317770203a1554260699994",
  "1.19.1": "90d438c3e432add8848a9f9f368ce5a52f6bc4a7",
  "1.19.3": "977727ec9ab8b4631e5c12839f064092f17663f8",
  "1.19.4": "958928a560c9167687bea0cefeb7375da1e552a8",
  "1.20": "e575a48efda46cf88111ba05b624ef90c520eef1",
  "1.20.1": "0c3ec587af28e5a785c0b4a7b8a30f9a8f78f838",
  "1.20.2": "82d1974e75fc984c5ed4b038e764e50958ac61a0",
  "1.20.3": "b178a327a96f2cf1c9f98a45e5588d654a3e4369",
  "1.20.4": "fd19469fed4a4b4c15b2d5133985f0e3e7816a8a",
  "1.20.5": "c6b92b2374a629f20802bb284f98a4ee790e950a",
  "1.20.6": "05b6f1c6b46a29d6ea82b4e0d42190e42402030f",
  "1.21": "0e9a07b9bb3390602f977073aa12884a4ce12431",
  "1.21.1": "30c73b1c5da787909b2f73340419fdf13b9def88",
  "1.21.2": "c7ac2d0d86f4ca416cab9064ff8a281852ad0c7b",
  "1.21.3": "6f67d19b4467240639cb2c368ffd4b94ba889705",
  "1.21.4": "a7e5a6024bfd3cd614625aa05629adf760020304",
  "1.21.5": "b88808bbb3da8d9f453694b5d8f74a3396f1a533",
  "1.21.6": "740a125b83dd3447feaa3c5e891ead7fbb21ae28",
  "1.21.7": "a2db1ea98c37b2d00c83f6867fb8bb581a593e07",
  "1.21.8": "a19d9badbea944a4369fd0059e53bf7286597576",
  "1.21.9": "ce92fd8d1b2460c41ceda07ae7b3fe863a80d045",
  "1.21.10": "d3bdf582a7fa723ce199f3665588dcfe6bf9aca8",
  "1.21.11": "ba2df812c2d12e0219c489c4cd9a5e1f0760f5bd",
  "26.1": "191771837687b766537a8c4607cb6fad79c533a1",
  "26.1.1": "377031a9e733ba8ab4d355959a8f6fb8eb707556",
  "26.2": "2dc72797acbc1b63fc16a11c4ac393605f453754",
  "26.3": "e877b6a07acd633fb3bb475002175cec036e7b87",
};

/**
 * The corrected minecraft-data pnpm-workspace.yaml links, with the data submodule the commit holds (`data`). The
 * repository is where a clean host clones it from; MC_MINECRAFT_DATA_SOURCE and MC_MINECRAFT_DATA_SUBMODULE_SOURCE
 * name another place holding the same commits (d0658d9 and its data e0313e69 are not yet on GitHub).
 */
const MINECRAFT_DATA = {
  repository: "https://github.com/mc-zuri/minecraft-data-node.git",
  commit: "d0658d964859debe63c9883e2b92259970058361",
  data: "e0313e690a3a3c4d2f5b62f35fcff0bf39ce8b0b",
};

// The clients of the releases read from a tree of their own: the legacy releases, decompiled from their
// NAMED_JARS jar (MC_LEGACY_SOURCES, scripts/materialize-sources.cjs), which unimined remapped from this client,
// and 26.2 and 26.3 (MC_SOURCE_ROOT). Each sha1 is the one the release's pinned version json declares.
const TREE_CLIENTS = {
  "1.8.8": "0983f08be6a4e624f5d85689d1aca869ed99c738",
  "1.9.4": "4a61c873be90bb1196d68dac7b29870408c56969",
  "1.10.2": "dc8e75ac7274ff6af462b0dcec43c307de668e40",
  "1.11.2": "db5aa600f0b0bf508aaf579509b345c4e34087be",
  "1.12.2": "0f275bc1547d01fa5f56ba34bdc87d981ee12daf",
  "1.13.2": "30bfe37a8db404db11c7edf02cb5165817afb4d9",
  "26.2": CLIENT_DATA["26.2"],
  "26.3": CLIENT_DATA["26.3"],
};

// The Mojang-named jars the oracles run (scripts/java-oracle/classpath.cjs) by sha256, for every release before 26.1;
// from 26.1 the client jar is Mojang-named and its sha1 is the pin. Mojang publishes no such jar: unimined remaps the
// client, from 1.14.4 by Mojang's mappings and before by legacy-mappings/<release>.tiny (the folder is named by
// that file's sha1). Its output stamps the time of the remap into every zip entry, so these bytes cannot be made again:
// they are pinned inputs, not reproductions. scripts/fetch-jar-inputs.cjs takes them only at these hashes, and a table
// built from one records it (generated/block-shapes.json remappedSha256).
const NAMED_JARS = {
  "1.8.8": "6b04ba67f26b024eca38da0b725c74707bbbfd452a19a9ff9f388cd9a9f95126",
  "1.9.4": "b51df795602cf7db13963b4c47a2aac468c733b13653f67764acb5d7feb0ce75",
  "1.10.2": "e675cda5ba5f17b95195ed6e9cab04ab6ec9c1bd9b7d23526d54a69297a7a1ca",
  "1.11.2": "57b0457a003b7bba8142897d0ae1cba5406519396bec31d5484a0161eb4b0672",
  "1.12.2": "4b13a30b9cec4e1f6ab3ef0fe6cb6eed1a6447f8bbafee2d99261f1174d1a6e2",
  "1.13.2": "634828d65d2212fe3b6e7e2e4a1befa5343b57ec7cdfbc36abf3ca7bf139236c",
  "1.14.4": "57f6115d73367b8a5d2dcd5d7230afa7d758108d36a3a411f4cf38beb4e51de4",
  "1.15": "aac72c6d121c7b4890f494df91c7b859a4ef623f37f34a4d0331250df67eaff0",
  "1.15.1": "9ad2444c74f090833e09641f396dfc14bcaa2ad2c84d675eb22802010e29bb11",
  "1.15.2": "474f05aa32df28632e3314287d5b4c7ec1d675e233749d1ed38ee350fc64e6f8",
  "1.16": "521a21a07e669542a7a626521f709b581a0f9014b3dad2f80da9a93d59a428b3",
  "1.16.1": "452f7aa006fae45df63e99363aa01cf98cd62172f04b7b1017581e3b1afa0168",
  "1.16.2": "1a962a763432a27484c6571f0f10eb98f1bfc124ece0c49f787a593e74954a72",
  "1.16.3": "b6a70751506db5ace1908982642eb57953f7b596151d5889c468e4333d55928e",
  "1.16.4": "0a1f709402db0ad07e554b99a6bb0ee58b01e454b1a7f10cac3e24c1727acfed",
  "1.16.5": "5ed9b54f988ba6021a0acdeb8b99d7f097300cc033b2bf30a87d5f1858f174ab",
  "1.17": "553ac7daf1185b3597861b3653447c1aff86e39b18ffdd01a20cd37d71dbfd6d",
  "1.17.1": "8df36660ba33f406ceafdc9d36f101c28a2bf123332fee425d745a9e1f2b3490",
  "1.18": "c95a274d63375a228d1cf689242b3abf87a2d3c452d46e48472f9134927dfc74",
  "1.18.1": "f3bbeb190090c23d6b5a07e634437b59fb5f79b76aab193617ea088b167d2c5e",
  "1.18.2": "cdc4f7eab0026b88ef5bf15a454826afc7d5bd342c0a84e9eee99f7d43b13ad7",
  "1.19": "9bb7f63e041f37ff9b6c552f076352bd3db42eb76890d72ca78506205ab77d79",
  "1.19.1": "5e6496ce1e16b11e6e98f88806daa388524288abc22055bed798eefb29c84b16",
  "1.19.2": "38d622befdc24edc77c2859bbaad549d2ce5717f8a33a77c20b0624a423a745b",
  "1.19.3": "b861870318cc422a6009564edc25c742a8883d7d14bfe623d5c5f98a97c3deb8",
  "1.19.4": "526af90d3233220ddce03f068135ea56b72f3ff7995d3d650e74f60090be365e",
  "1.20": "0214b75eafb3f8e4fba2fd94b5393f870e5c2fdacccd60da63c0c5ef92ef9ab2",
  "1.20.1": "77f7d68d574c44e1e491c75e8d6c2d7138237fa5d9a48aacb2397cb7a2aec17e",
  "1.20.2": "2911727725e55c82ab7fde82ed60260bbb74eb15d1e80a1d7dfef5700d941868",
  "1.20.3": "12a552c1478dc1abc3e76b957eb51ff442fc1e26c2280c186e51452018126b62",
  "1.20.4": "df190ca87cb73fe4318630ff61493a238ecd136bec276b664375ff58040a55ff",
  "1.20.5": "cb10c31df530e95ca25a1bca422153d2065672a02ef05534c541fc5659ed3db4",
  "1.20.6": "839af86f74592b588448875729ae527868f4f02c99ba5893b2ae4dd93e8b691c",
  "1.21": "aa1ef382eb46317fb59674129b34733eb77ea9473533381b1f17cd7369017553",
  "1.21.1": "6dd8f5fe6b2fee4266c17ac69e06f0e05542c7aaa27b1a60a8a72ee01380b13a",
  "1.21.2": "6a707e9c047fa0d41b7c1d5ff1afa63d52b6f21840afcc0f3bb8daf6b12ad820",
  "1.21.3": "b37504a520620f64a1be031731cbae5502f0eb4bf96d05dc8491293edf03713e",
  "1.21.4": "6fe1674efe765083b09d183c819afd730fc9d1f013fa389defb5f9bbe85c1305",
  "1.21.5": "48ed5192eba44e95203544666810d13fd882a092de1f83874813f5e62f9a0063",
  "1.21.6": "b9d5746178e3d2f12ae236b73e2351ffdb8737996ebedbe89aad60c175d1f489",
  "1.21.7": "834aa04bad78770bb121ccc5ee7439f4b9a09407f5bbd0dc26a674be6991b52b",
  "1.21.8": "5843e4a3ad55cc5a08a9fc3b085d8a8d0313c8fb68d9732a55839d9c932d1574",
  "1.21.9": "519bdcc940acfe20eefe65a7df93c35852523477207c1055f5fbf496283f093b",
  "1.21.10": "2b1ac314b250a144792a7282ecfec68ef6846c45ca5be4795a00ef173c7c5ae0",
  "1.21.11": "45bf0c11c054fbba7e9bf18b4d6cc6635c486ea24bcfbbfebaf9807e64dbcb43",
};

/** Where a release's Mojang-named jar is under MC_CLIENT_JAR_ROOT, in unimined's layout, `/`-separated. */
function namedJarPath(version) {
  if (!NAMED_JARS[version]) return `${version}/minecraft-${version}-client.jar`;
  const tail = "fabric-merged+custom-no-environment+fixed";
  if (Number(version.split(".")[1]) >= 14) return `${version}/intermediary-${version}-mojmap-${version}/minecraft-${version}-${tail}-mojmap+intermediary.jar`;
  // unimined names the folder by the sha1 of the mapping file as checked out, so a Linux checkout (LF) and a Windows one
  // (CRLF) name different folders for one mapping: the pinned jars were remapped on Windows, from CRLF bytes
  const text = fs.readFileSync(path.join(repositoryRoot, "legacy-mappings", `${version}.tiny`), "latin1").replaceAll("\r\n", "\n");
  const mapping = crypto.createHash("sha1").update(Buffer.from(text.replaceAll("\n", "\r\n"), "latin1")).digest("hex");
  return `${version}/intermediary-${version}-mojang-${version}-${mapping.slice(0, 7)}/minecraft-${version}-${tail}-named+intermediary.jar`;
}

/** The decompiler the sources of a pinned client come from, by its own sha1. */
const DECOMPILER = { name: "vineflower", version: "1.12.0", sha1: "85570609a0a5941a7d2918b6260b209de810f66f" };

const clientJarRoot = () => process.env.MC_CLIENT_JAR_ROOT
  ?? path.join(os.homedir(), ".gradle", "caches", "unimined", "net", "minecraft", "minecraft");
const decompilerJar = () => process.env.MC_DECOMPILER_JAR ?? path.join(os.homedir(), ".gradle", "caches", "modules-2",
  "files-2.1", "org.vineflower", "vineflower", DECOMPILER.version, DECOMPILER.sha1, `vineflower-${DECOMPILER.version}.jar`);
/** What a recorded `<pin>:<path>` starts with for a release: its commit, else its client's sha1. */
const pinOf = (version) => PINNED_SOURCES[version] ?? PINNED_CLIENTS[version]?.client ?? null;

const extractedRepo = () => process.env.MC_EXTRACTED_DATA_REPO ?? "D:/projects/mc-zuri/mc-data/extracted_minecraft_data";
const mcSourceRoot = () => process.env.MC_SOURCE_ROOT ?? path.join(repositoryRoot, "..", "..", "pr", "mc-source");
const sourceCache = () => process.env.MC_SOURCE_CACHE ?? path.join(repositoryRoot, "..", "source-cache");

module.exports = { PINNED_SOURCES, PINNED_ROOT_SOURCES, PINNED_CLIENTS, CLIENT_DATA, TREE_CLIENTS, NAMED_JARS, namedJarPath, DECOMPILER, MINECRAFT_DATA, clientJarRoot, decompilerJar, pinOf, extractedRepo, sourceCache, mcSourceRoot };
