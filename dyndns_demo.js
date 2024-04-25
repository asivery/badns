bindAddress("0.0.0.0", 53535);
// No upstream servers

const OWN_ROOT = ["dyn", "domain", "tld"];
const OWN_ROOT_J = OWN_ROOT.join('.');
const KEYS = {
    subdomain: 'secret_key1',
}
let dynamicBindings = {};

function checkIfValid(nameTokens){
    if(nameTokens.length <= OWN_ROOT.length) return false;
    return OWN_ROOT.every((s, i) => s === nameTokens[nameTokens.length - OWN_ROOT.length + i]);
}

function getFromLast(tokens, i){
    const idx = tokens.length - OWN_ROOT.length - i - 1;
    if(idx < 0 || idx >= tokens.length) return null;
    return tokens[idx];
}

addUniversalBinding(function getDynamic(name, rrtype){
    const tokens = name.split(".");
    if(!checkIfValid(tokens)) return null;
    if(tokens.length !== OWN_ROOT.length + 1) return null;
    if(rrtype !== RR_A) return null;

    const record = dynamicBindings[getFromLast(tokens, 0)];
    if(record === undefined){
        return (
            {
                type: "CNAME",
                ttl: 0,
                target: 'unbound.' + OWN_ROOT_J,
                authoritative: true,
            }
        );
        
    }

    return [
        {
            type: "A",
            ttl: 3600,
            ip: record,
            authoritative: true,
        }
    ]
})

addUniversalBinding(function configure(name, rrtype, rrclass, peerAddress){
    const tokens = name.split(".");
    // If valid request...
    if(!checkIfValid(tokens)) return null;
    // If we're configuring...
    if(getFromLast(tokens, 1) !== "configure") return null;
    const domain = getFromLast(tokens, 0);
    // If allowed domain...
    if(!(domain in KEYS)) return null;

    // If has all required parameters
    const timestamp = getFromLast(tokens, 2);
    const checksum = getFromLast(tokens, 3);
    if(!(timestamp && checksum)) return null;

    // If timestamp not too far off from current time...
    const currentTimestamp = Math.floor(new Date().getTime() / 1000);
    const providedTimestamp = parseInt(timestamp);
    if(Math.abs(providedTimestamp - currentTimestamp) > 60) return null;
    
    const key = KEYS[domain];
    const toSha = key + '/' + timestamp;
    const validChecksum = sha256(toSha).substring(0, 16);

    // If checksums match
    if(validChecksum.toLowerCase() !== checksum.toLowerCase()) return null;

    // Set the dynamic remap table
    if(peerAddress.includes(':')) peerAddress = peerAddress.substring(0, peerAddress.indexOf(":"));
    dynamicBindings[domain] = peerAddress;
    console.log(`Updated dynamic record for domain ${domain} to ${peerAddress}`);
    return [
        {
            type: "CNAME",
            ttl: 0,
            target: "ok." + OWN_ROOT_J,
        },
    ];
});

addABinding("unbound." + OWN_ROOT_J, STUB);