// Main baDNS init file. This file gets executed before the config file is evaluated.
// Its aim is to prepare the environment and expose more user-friendly methods to the
// config file. It also emulates some standard JS functions, such as `console.log`.
// All fields and functions whose name is prefixed with `badns_` are part of the bridge
// between rust and JS. They shouldn't be directly interfaced with. Instead use the
// functions exposed by this file in your config file.
// The functions described below assume the following types:
// 
// type RRConstant = RR_A | RR_AAAA | RR_CNAME
// type Handler = (name: string, rrtype: RRConstant, rrclass: number, peerAddress: string, ownAddress: string) => Response[] | Response
// type Response = NormalResponse | SpecialResponse;
// type SpecialType = 'queryUpstream';
// type NormalResponseType = 'A' | 'AAAA' | 'CNAME'
// interface SpecialResponse {
//     special: true,
//     specialType: SpecialType
// }
// interface QueryUpstreamSpecialResponse implements SpecialResponse{
//     name: string,
//     rrtype: RRConstant,
//     rrclass: Number,
// }
// interface NormalResponse{
//     ttl: number,
//     type: NormalResponseType,
// }
// interface AResponse implements NormalResponse {
//     ip: string,
// }
// interface AAAAResponse implements NormalResponse {
//     ip: string,
// }
// interface CNAMEResponse implements NormalResponse {
//     target: string,
// };
// 
// The available non-internal functions are:
// - [1] bindAddress(address: string, port = 53) => undefined
//   Binds the UDP address and starts listening on it.
//   There can be multiple interfaces open at once.
//
// - [1] upstream(address: string, port = 53) => undefined
//   Adds an upstream server. If the JS config doesn't have a response for a given question,
//   the baDNS server will query all upstream servers in the order they were added in until it finds
//   one with at least one answer
//
// - [1] setupHTTPRedirectServer(address: string, port: number, recordTarget = ip) => undefined
//   Sets up the HTTP reverse proxy. The HTTP server will bind on `address:port`.
//
// - [1] addHTTPRedirect(target: string, name: string) => undefined
//   Adds a reverse proxy entry, which will bind the domain `name` to HTTP address `target`.
//   Target should follow the standard format of `address:port`.
// 
// - addBinding(rrtype: RRConstant, name: string, handler: Handler) => undefined
//   Adds a binding for a given rrtype AND name. Once a request that matches both occurs, handler will get triggered
//
// - addABinding(name: string, handler: Handler) => undefined
//   Adds a RR_A binding using addBinding()
// 
// - addAAAABinding(name: string, handler: Handler) => undefined
//   Adds a RR_AAAA binding using addBinding()
// 
// - addCNAMEBinding(name: string, handler: Handler) => undefined
//   Adds a RR_CNAME binding using addBinding()
// 
// - addUniversalBinding(handler: Handler) => undefined
//   Adds a universal binding that will get triggered on every query, assuming a named
//   handler (one added via getBinding or derivatives) doesn't get triggered first
// 
// - STUB() => AResponse
//   Returns an RR_A response with an infinite TTL that points to 0.0.0.0
//
// - permanentBinding(ip: string, domain: string) => undefined
//   Calls addABinding with a handler that always returns an RR_A response with the given IP
//
// - ban(domain: string) => undefined
//   Calls addABinding with a STUB() handler for domain provided
//
// - exec(filename: string) => any
//   Evaluates the contents of the file passed as the argument
//
//   -----------------------------baDNS extensions-----------------------------
// 
// - sha256(data: string) => string
//   Generates a SHA256 digest of the data passed in as the argument
//
// - readFile(filename: string) => string
//   Reads the file whose path was provided as the argument, and returns its contents parsed as UTF8
//
// [1] - Can only be executed on initial loading of the config file.


// =========================================== Core methods ============================================
const log = (...e) => badns_log(e.map(q => q === undefined ? '<undefined>' : q === null ? '<null>' : q.toString()).join("\n"));
const console = { log };


// ================================= Rust-exposed functions and fields =================================
function badns_getResponse(name, rrtype, rrclass, peerAddress, ownAddress) {
    log(`Requested JS response for ${name} (${RRrevs[rrtype]})`);
    // Construct a terrible name in the bindings
    const bindingName = rrtype + "_" + name;
    const potentialResponders = [bindings[bindingName], ...unnamedBindings];
    for (let responder of potentialResponders){
        if(!responder) continue;
        let response = responder?.(name, rrtype, rrclass, peerAddress, ownAddress) ?? null;
        if (response) {
            if(!Array.isArray(response)){
                const recursedCName = response.type === "CNAME" ? recurse(response.target, rrtype, rrclass, peerAddress, ownAddress) : [];
                response = [ response, ...recursedCName ];
            }
            log(`Responder ${responder.name || '<anon>'} replied!`);
            if (!response.every(e => validateResponse(e))){
                log("Validation fail - returning nonexistent");
                return '[]';
            }
            return JSON.stringify(response);
        }
    }
    return '[]';
}

let badns_httpRedirectHost = "";
let badns_httpRedirectPort = 0;
let badns_afterInit = false;

let badns_httpRedirectRecordTarget = "127.0.0.1";

// ==================================== Low-level initializing APIs ====================================
function bindAddress(address, port){
    assertInitIsntComplete();
    badns_bindAddress(address, port || 53);
}

function upstream(address, port){
    assertInitIsntComplete();
    badns_upstream(address, port || 53);
}

function setupHTTPRedirectServer(ip, port, recordTarget = undefined){
    assertInitIsntComplete();
    badns_httpRedirectHost = ip;
    badns_httpRedirectPort = port;
    badns_httpRedirectRecordTarget = recordTarget ?? ip;
}

function addHTTPRedirect(target, name){
    assertInitIsntComplete();
    if(badns_afterInit){
        throw Error("HTTP Server deployed and frozen - cannot alter state!");
    }

    if(badns_httpRedirectPort === 0){
        log("A HTTP redirection is being added, but HTTP redirect service isn't configured!");
    }

    // Assert target is a URL:
    if(!target.startsWith("http://") && !target.startsWith("https://")) {
        throw Error("The first parameter must be the target's URL!");
    }
    badns_setHTTPRedirect(name, target);
    addABinding(name, () => ({ type: "A", ttl: 100, ip: badns_httpRedirectRecordTarget }));
}

// =============================== Internal init.js functions and storage ==============================

const bindings = {};
const unnamedBindings = [];

function assertInitIsntComplete(){
    if(badns_afterInit){
        throw Error("Server is in post-initialization state! Cannot redefine basic parameters!");
    }
}

function recurse(name, rrtype, rrclass, peerAddress, ownAddress){
    const own = JSON.parse(badns_getResponse(name, rrtype, rrclass, peerAddress, ownAddress));
    if(own.length) return own;
    return [{
        special: true,
        specialType: 'queryUpstream',
        name, rrtype, rrclass
    }];
}

function validateResponse(response) {
    // Check 1 - is an object
    if (typeof response !== 'object') {
        log("Validate: Not an object!");
        return false;
    }

    if(response.special) return true; // Special commands work differently.

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


// =========================================== Ease of life ============================================

function addBinding(rrtype, name, handler) {
    const bindingName = rrtype + "_" + name;
    bindings[bindingName] = handler;
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
    unnamedBindings.push(handler);
}

function STUB(){
    return {
        "type": "A",
        "ip": "0.0.0.0",
        "ttl": 99999999,
    };
}

function permanentBinding(ip, domain){
    addABinding(domain, () => ({
        "type": "A",
        "ttl": 99999999,
        ip
    }));
}

function ban(target){
    addABinding(target, STUB);
}

function exec(fname){
    eval(readFile(fname));
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

'special' bindings:
{
    special: true,
    specialType: 'queryUpstream',
    ... // Type-dependent parameters
}

All responses can have the following values:
- authoritative
*/
