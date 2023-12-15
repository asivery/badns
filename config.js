bindAddress("0.0.0.0", 1234);
bindAddress("0.0.0.0", 5432);
upstream("1.1.1.1");

addABinding("example.net", function(name){
    return [{
        "type": "CNAME",
        "target": "example.com",
        "ttl": 100
    }];
});

