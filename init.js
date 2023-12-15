const log = (...e) => badns_log(e.join("\n"));
const console = { log };

function bindAddress(address, port){
    badns_bindAddress(address, port || 53);
}

function upstream(address, port){
    badns_upstream(address, port || 53);
}

function queryUpstream(){
    console.log("!!! QueryUpstream !!!");
    return [];
}

let badns_httpRedirectHost = "";
let badns_httpRedirectPort = 0;
let badns_httpFrozen = false;

let badns_httpRedirectRecordTarget = "127.0.0.1";

function setupHTTPRedirectServer(ip, port, recordTarget = undefined){
    if(badns_httpFrozen){
        throw Error("HTTP Server deployed and frozen - cannot alter state!");
    }
    badns_httpRedirectHost = ip;
    badns_httpRedirectPort = port;
    badns_httpRedirectRecordTarget = recordTarget ?? ip;
}

function addHTTPRedirect(target, name){
    if(badns_httpFrozen){
        throw Error("HTTP Server deployed and frozen - cannot alter state!");
    }

    if(badns_httpRedirectPort === 0){
        log("A HTTP redirection is being added, but HTTP redirect service isn't configured!");
    }

    badns_setHTTPRedirect(name, target);
    addABinding(name, () => ({ type: "A", ttl: 100, ip: badns_httpRedirectRecordTarget }));
}

const badns_bindings = {};
const badns_nonNamedBindings = [];

function recurse(name, rrtype, rrclass, peerAddress, ownAddress){
    const own = JSON.parse(badns_getResponse(name, rrtype, rrclass, peerAddress, ownAddress));
    if(own.length) return own;
    return queryUpstream(name, rrtype, rrclass);
}

function badns_getResponse(name, rrtype, rrclass, peerAddress, ownAddress) {
    log(`Requested JS response for ${name} (${RRrevs[rrtype]})`);
    // Construct a terrible name in the bindings
    const bindingName = rrtype + "_" + name;
    const potentialResponders = [badns_bindings[bindingName], ...badns_nonNamedBindings];
    for (let responder of potentialResponders){
        if(!responder) continue;
        let response = responder?.(name, rrtype, rrclass, peerAddress, ownAddress) ?? null;
        if (response) {
            if(!Array.isArray(response)){
                const recursedCName = response.type === "CNAME" ? recurse(response.target, rrtype, rrclass, peerAddress, ownAddress) : [];
                response = [ response, ...recursedCName ];
            }
            log(`Responder ${responder.name || '<anon>'} replied!`);
            if (!response.every(e => badns_validateResponse(e))){
                log("Validation fail - returning nonexistent");
                return '[]';
            }
            return JSON.stringify(response);
        }
    }
    return '[]';
}

function badns_validateResponse(response) {
    // Check 1 - is an object
    if (typeof response !== 'object') {
        log("Validate: Not an object!");
        return false;
    }

    const globalRequiredFieldsAndTypes = {
        'ttl': 'number',
        'type': (type) => ['A', 'AAAA', 'CNAME'].includes(type),
    };

    function _validate(object, template) {
        for (let [name, type] of Object.entries(template)) {
            if (object[name] === undefined) return false;
            if (typeof type === 'function') return type(object[name]);
            return typeof object[name] === type;
        }
    }

    if (!_validate(response, globalRequiredFieldsAndTypes)) {
        log(`Generic contents: required fields ${Object.keys(globalRequiredFieldsAndTypes)}`);
        return false;
    }

    if (response.type === 'A' || response.type === 'AAAA') {
        const aFields = {
            'ip': 'string'
        };
        if (!_validate(response, aFields)) {
            log(`A contents: required fields ${Object.keys(aFields)}`);
            return false;
        }
    } else if(response.type === 'CNAME') {
        const aFields = {
            'target': 'string',
        };
        if (!_validate(response, aFields)) {
            log(`CNAME contents: required fields ${Object.keys(aFields)}`);
            return false;
        }
    }

    return true;
}

function addBinding(rrtype, name, handler) {
    const bindingName = rrtype + "_" + name;
    badns_bindings[bindingName] = handler;
}

function addABinding(name, handler) {
    addBinding(RR_A, name, handler);
}

function addAAAABinding(name, handler) {
    addBinding(RR_AAAA, name, handler);
}

function addCNAMEBinding(name, handler) {
    addBinding(RR_CNAME, name, handler);
}

function addUniversalBinding(handler) {
    badns_nonNamedBindings.push(handler);
}

function STUB(){
    return {
        "type": "A",
        "ip": "0.0.0.0",
        "ttl": 99999999,
    };
}

function permanentBinding(ip, domain){
    addABinding(domain, {
        "type": "A",
        "ttl": 99999999,
        ip
    });
}

function ban(target){
    addABinding(target, STUB);
}

/*
baDNS response type:
For A / AAAA bindings: 
{
    ttl: number,
    type: 'A' | 'AAAA',
    'ip': string,
}

For CNAME bindings:
{
    ttl: number,
    type: 'CNAME',
    'target': FQDN,
}
*/
