Verified actual package path
From ./jira, @compiled/babel-plugin resolves to:

/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/@compiled/babel-plugin/dist/index.js
When that package requires @compiled/css, Node resolves:

/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/@compiled/babel-plugin/node_modules/@compiled/css/dist/index.js
Package version:

@compiled/css 0.19.0
So the actual Jira Compiled path is:

@compiled/parcel-transformer 0.18.4
-> @compiled/babel-plugin 0.36.1
-> nested @compiled/css 0.19.0
Verified runtime-loaded packages
The runtime instrumentation saw:

@compiled/babel-plugin 0.36.1
/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/@compiled/babel-plugin/package.json

@compiled/css 0.19.0
/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/@compiled/babel-plugin/node_modules/@compiled/css/package.json

autoprefixer 10.4.14
/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/autoprefixer/package.json

browserslist 4.24.2
/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/browserslist/package.json

postcss 8.5.6
/home/ubuntu/atlassian-frontend-monorepo/jira/node_modules/postcss/package.json
Verified @compiled/css@0.19.0 transform code
The actual nested file:

jira/node_modules/@compiled/babel-plugin/node_modules/@compiled/css/dist/transform.js
does:

...(process.env.AUTOPREFIXER === 'off' ? [] : [autoprefixer()])
and then:

.process(css, {
from: undefined,
});
So it calls:

autoprefixer()
with no args.
Verified runtime Browserslist call
At runtime, through the actual @compiled/css@0.19.0 path, Autoprefixer called Browserslist as:

browserslist(null, {
path: "/home/ubuntu/atlassian-frontend-monorepo/jira"
})
Not with mobileToDesktop: true.
That resolved to:

[
"and_chr 144",
"chrome 144",
"chrome 143",
"chrome 142",
"chrome 141",
"chrome 140",
"edge 144",
"edge 143",
"firefox 147",
"firefox 146",
"ios_saf 26.2",
"ios_saf 26.1",
"safari 26.2",
"safari 26.1"
]
Corrected final Browserslist for Jira Compiled stage
For the actual Jira build path:

Parcel transformer
-> @compiled/babel-plugin
-> nested @compiled/css@0.19.0
-> autoprefixer@10.4.14
-> browserslist@4.24.2
the effective browser list is:

and_chr 144
chrome 144
chrome 143
chrome 142
chrome 141
chrome 140
edge 144
edge 143
firefox 147
firefox 146
ios_saf 26.2
ios_saf 26.1
safari 26.2
safari 26.1
No and_chr 143 in the actual nested @compiled/css@0.19.0 path.
Corrected byte-equality package set
Use these, not the top-level @compiled/css@0.21.1:

@compiled/parcel-transformer  0.18.4
@compiled/babel-plugin        0.36.1
@compiled/css                 0.19.0
autoprefixer                  10.4.14
browserslist                  4.24.2
postcss                       8.5.6
And the exact @compiled/css path is:


babel/core 7.28.5
@babel/parser 7.29.0
@babel/generator 7.23.0
@babel/template 7.28.6
@babel/traverse 7.29.0
@babel/types 7.29.0